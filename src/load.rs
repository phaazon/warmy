//! Load and reload resources.
//!
//! This module exposes traits, types and functions you need to use to load and reload objects.

use any_cache::{Cache, HashCache};
use notify::{self, DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::fmt::{self, Display};
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver};
use std::time::Duration;

use crate::key::{Key, PrivateKey};
use crate::res::Res;

/// Class of types that can be loaded and reloaded.
///
/// The first type variable, `C`, represents the context of the loading. This will be accessed via
/// a mutable reference when loading and reloading.
///
/// The second type variable, `K`, is the type of key that can be used to index resources. Some
/// special resource keys exist:
///
///   - [`SimpleKey`]: such a key indexes a resource that lives either on the filesystem or as a
///     logical resource (in-memory, on-the-fly, etc.)
///
/// A key type must implement the [`Key`] trait in order to be usable.
///
/// The last type variable, `Method`, is a tag-only value that is useful to implement several
/// algorithms to load the same type with different methods.
///
/// [`SimpleKey`]: crate::key::SimpleKey
pub trait Load<C, K, Method = ()>: 'static + Sized
where
  K: Key,
  Method: ?Sized, {
  /// Type of error that might happen while loading.
  type Error: Display + 'static;

  /// Load a resource.
  ///
  /// The [`Storage`] can be used to load additional resource dependencies.
  ///
  /// The result type is used to register for dependency events. If you do not need any, you can
  /// lift your return value in [`Loaded`] with `your_value.into()`.
  fn load(key: K, storage: &mut Storage<C, K>, ctx: &mut C)
    -> Result<Loaded<Self, K>, Self::Error>;

  // FIXME: add support for redeclaring the dependencies?
  /// Function called when a resource must be reloaded.
  ///
  /// The default implementation of that function calls [`Load::load`] and returns its result.
  fn reload(&self, key: K, storage: &mut Storage<C, K>, ctx: &mut C) -> Result<Self, Self::Error> {
    Self::load(key, storage, ctx).map(|lr| lr.res)
  }
}

/// Result of a resource loading.
///
/// This type enables you to register a resource for reloading events of other resources. Those are
/// named dependencies. If you don’t need to run specific code on a dependency reloading, use
/// the `.into()` function to lift your return value to [`Loaded`] or use the provided
/// [`Loaded::without_dep`] function.
pub struct Loaded<T, K> {
  /// The loaded object.
  pub res: T,
  /// The list of dependencies to listen for events.
  pub deps: Vec<K>,
}

impl<T, K> Loaded<T, K> {
  /// Return a resource declaring no dependency at all.
  pub fn without_dep(res: T) -> Self {
    Loaded {
      res,
      deps: Vec::new(),
    }
  }

  /// Return a resource along with its dependencies.
  pub fn with_deps(res: T, deps: Vec<K>) -> Self {
    Loaded { res, deps }
  }
}

impl<T, K> From<T> for Loaded<T, K> {
  fn from(res: T) -> Self {
    Loaded::without_dep(res)
  }
}

/// Metadata about a resource.
struct ResMetaData<C, K> {
  /// Function to call each time the resource must be reloaded.
  on_reload: Box<dyn Fn(&mut Storage<C, K>, &mut C) -> Result<(), Box<dyn Display>>>,
}

impl<C, K> ResMetaData<C, K> {
  fn new<F>(f: F) -> Self
  where F: 'static + Fn(&mut Storage<C, K>, &mut C) -> Result<(), Box<dyn Display>> {
    ResMetaData {
      on_reload: Box::new(f),
    }
  }
}

/// Resource storage.
///
/// This type is responsible for storing resources, giving functions to look them up and update
/// them whenever needed.
pub struct Storage<C, K> {
  // canonicalized root path (used for resources loaded from the file system)
  canon_root: PathBuf,
  // resource cache, containing all living resources
  cache: HashCache,
  // dependencies, mapping a dependency to its dependent resources
  deps: HashMap<K, Vec<K>>,
  // contains all metadata on resources (reload functions)
  metadata: HashMap<K, ResMetaData<C, K>>,
}

impl<C, K> Storage<C, K>
where K: Key
{
  fn new(canon_root: PathBuf) -> Self {
    Storage {
      canon_root,
      cache: HashCache::new(),
      deps: HashMap::new(),
      metadata: HashMap::new(),
    }
  }

  /// The canonicalized root the [`Storage`] is configured with.
  pub fn root(&self) -> &Path {
    &self.canon_root
  }

  /// Inject a new resource in the store.
  ///
  /// The resource might be refused for several reasons. Further information in the documentation of
  /// the [`StoreError`] error type.
  fn inject<T, M>(&mut self, key: K, resource: T, deps: Vec<K>) -> Result<Res<T>, StoreError<K>>
  where T: Load<C, K, M> {
    // we forbid having two resources sharing the same key
    if self.metadata.contains_key(&key) {
      return Err(StoreError::AlreadyRegisteredKey(key));
    }

    // wrap the resource to make it shared mutably
    let res = Res::new(resource);

    // create the metadata for the resource
    let res_ = res.clone();
    let key_ = key.clone();
    let metadata = ResMetaData::new(move |storage, ctx| {
      let reloaded = <T as Load<C, K, M>>::reload(&res_.borrow(), key_.clone(), storage, ctx);

      match reloaded {
        Ok(r) => {
          // replace the current resource with the freshly loaded one
          *res_.borrow_mut() = r;
          Ok(())
        }
        Err(e) => Err(Box::new(e)),
      }
    });

    self.metadata.insert(key.clone(), metadata);

    // register the resource as an observer of its dependencies in the dependencies graph
    let root = &self.canon_root;
    for dep in deps {
      self
        .deps
        .entry(dep.clone().prepare_key(root))
        .or_insert_with(Vec::new)
        .push(key.clone());
    }

    // wrap the key in our private key so that we can use it in the cache
    let pkey = PrivateKey::new(key);

    // cache the resource
    self.cache.save(pkey, res.clone());

    Ok(res)
  }

  /// Get a resource from the [`Storage`] and return an error if its loading failed.
  ///
  /// This function uses the default loading method.
  pub fn get<T>(&mut self, key: &K, ctx: &mut C) -> Result<Res<T>, StoreErrorOr<T, C, K>>
  where T: Load<C, K> {
    self.get_by(key, ctx, ())
  }

  /// Get a resource from the [`Storage`] by using a specific method and return and error if its
  /// loading failed.
  pub fn get_by<T, M>(
    &mut self,
    key: &K,
    ctx: &mut C,
    _: M,
  ) -> Result<Res<T>, StoreErrorOr<T, C, K, M>>
  where
    T: Load<C, K, M>,
  {
    let key = key.clone().prepare_key(self.root());

    // move the key into pkey to prevent an allocation and remove it after use
    let pkey = PrivateKey::<K, T>::new(key);
    let x: Option<Res<T>> = self.cache.get(&pkey).cloned();
    let key = pkey.0;

    match x {
      Some(resource) => Ok(resource),
      None => {
        let loaded =
          <T as Load<C, K, M>>::load(key.clone(), self, ctx).map_err(StoreErrorOr::ResError)?;
        self
          .inject::<T, M>(key, loaded.res, loaded.deps)
          .map_err(StoreErrorOr::StoreError)
      }
    }
  }

  /// Get a resource from the [`Storage`] for the given key. If it fails, a proxied version is used,
  /// which will get replaced by the resource once it’s available and reloaded.
  ///
  /// This function uses the default loading method.
  pub fn get_proxied<T, P>(
    &mut self,
    key: &K,
    proxy: P,
    ctx: &mut C,
  ) -> Result<Res<T>, StoreError<K>>
  where
    T: Load<C, K>,
    P: FnOnce() -> T,
  {
    self
      .get(key, ctx)
      .or_else(|_| self.inject::<T, ()>(key.clone(), proxy(), Vec::new()))
  }

  /// Get a resource from the [`Storage`] for the given key by using a specific method. If it fails, a
  /// proxied version is used, which will get replaced by the resource once it’s available and
  /// reloaded.
  pub fn get_proxied_by<T, M, P>(
    &mut self,
    key: &K,
    proxy: P,
    ctx: &mut C,
    method: M,
  ) -> Result<Res<T>, StoreError<K>>
  where
    T: Load<C, K, M>,
    P: FnOnce() -> T,
  {
    self
      .get_by(key, ctx, method)
      .or_else(|_| self.inject::<T, M>(key.clone(), proxy(), Vec::new()))
  }
}

/// Error that might happen when handling a resource store around.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum StoreError<K> {
  /// The root path for a filesystem resource was not found.
  RootDoesNotExist(PathBuf),
  /// The key associated with a resource already exists in the [`Store`].
  ///
  /// > Note: it is not currently possible to have two resources living in a [`Store`] and using an
  /// > identical key at the same time.
  AlreadyRegisteredKey(K),
}

impl<K> Display for StoreError<K>
where K: Display
{
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      StoreError::RootDoesNotExist(ref path) => write!(f, "root {} doesn’t exist", path.display()),
      StoreError::AlreadyRegisteredKey(ref dk) => write!(f, "already registered key: {}", dk),
    }
  }
}

/// Either a store error or a resource loading error.
pub enum StoreErrorOr<T, C, K, M = ()>
where
  T: Load<C, K, M>,
  K: Key, {
  /// A store error.
  StoreError(StoreError<K>),
  /// A resource error.
  ResError(T::Error),
}

impl<T, C, K, M> Clone for StoreErrorOr<T, C, K, M>
where
  T: Load<C, K, M>,
  T::Error: Clone,
  K: Key,
{
  fn clone(&self) -> Self {
    match *self {
      StoreErrorOr::StoreError(ref e) => StoreErrorOr::StoreError(e.clone()),
      StoreErrorOr::ResError(ref e) => StoreErrorOr::ResError(e.clone()),
    }
  }
}

impl<T, C, K, M> Eq for StoreErrorOr<T, C, K, M>
where
  T: Load<C, K, M>,
  T::Error: Eq,
  K: Key,
{
}

impl<T, C, K, M> PartialEq for StoreErrorOr<T, C, K, M>
where
  T: Load<C, K, M>,
  T::Error: PartialEq,
  K: Key,
{
  fn eq(&self, rhs: &Self) -> bool {
    match (self, rhs) {
      (&StoreErrorOr::StoreError(ref a), &StoreErrorOr::StoreError(ref b)) => a == b,
      (&StoreErrorOr::ResError(ref a), &StoreErrorOr::ResError(ref b)) => a == b,
      _ => false,
    }
  }
}

impl<T, C, K, M> fmt::Debug for StoreErrorOr<T, C, K, M>
where
  T: Load<C, K, M>,
  T::Error: fmt::Debug,
  K: Key + fmt::Debug,
{
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      StoreErrorOr::StoreError(ref e) => f.debug_tuple("StoreError").field(e).finish(),
      StoreErrorOr::ResError(ref e) => f.debug_tuple("ResError").field(e).finish(),
    }
  }
}

impl<T, C, K, M> Display for StoreErrorOr<T, C, K, M>
where
  T: Load<C, K, M>,
  T::Error: fmt::Debug,
  K: Key + Display,
{
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      StoreErrorOr::StoreError(ref e) => e.fmt(f),
      StoreErrorOr::ResError(ref e) => e.fmt(f),
    }
  }
}

/// Resource synchronizer.
///
/// An object of this type is responsible to synchronize resources living in a store. It keeps in
/// internal, optimized state to perform correct and efficient synchronization.
struct Synchronizer<C, K> {
  // all the resources that must be reloaded; they’re mapped to the instant they were found updated
  dirties: HashSet<K>,
  // keep the watcher around so that we don’t have it disconnected
  #[allow(dead_code)]
  watcher: RecommendedWatcher,
  // watcher receiver part of the channel
  watcher_rx: Receiver<DebouncedEvent>,
  // used to accept or ignore new discoveries
  discovery: Discovery<C, K>,
}

impl<C, K> Synchronizer<C, K>
where K: Key
{
  fn new(
    watcher: RecommendedWatcher,
    watcher_rx: Receiver<DebouncedEvent>,
    discovery: Discovery<C, K>,
  ) -> Self
  {
    Synchronizer {
      dirties: HashSet::new(),
      watcher,
      watcher_rx,
      discovery,
    }
  }

  /// Dequeue any file system events.
  fn dequeue_fs_events(&mut self, storage: &mut Storage<C, K>, ctx: &mut C)
  where K: for<'a> From<&'a Path> {
    for event in self.watcher_rx.try_iter() {
      match event {
        DebouncedEvent::Write(ref path) | DebouncedEvent::Create(ref path) => {
          let key = path.as_path().into();

          if storage.metadata.contains_key(&key) {
            self.dirties.insert(key);
          } else {
            self.discovery.discover(path, storage, ctx);
          }
        }

        _ => (),
      }
    }
  }

  /// Reload any dirty resource that fulfill its time predicate.
  fn reload_dirties(&mut self, storage: &mut Storage<C, K>, ctx: &mut C) {
    self.dirties.retain(|dep_key| {
      if let Some(metadata) = storage.metadata.remove(&dep_key) {
        if (metadata.on_reload)(storage, ctx).is_ok() {
          // if we have successfully reloaded the resource, notify the observers that this
          // dependency has changed
          if let Some(deps) = storage.deps.get(&dep_key).cloned() {
            for dep in deps {
              if let Some(obs_metadata) = storage.metadata.remove(&dep) {
                // FIXME: decide what to do with the result (error?)
                let _ = (obs_metadata.on_reload)(storage, ctx);

                // reinject the dependency once afterwards
                storage.metadata.insert(dep, obs_metadata);
              }
            }
          }
        }

        storage.metadata.insert(dep_key.clone(), metadata);
      }

      false
    });
  }

  /// Synchronize the [`Storage`] by updating the resources that ought to.
  fn sync(&mut self, storage: &mut Storage<C, K>, ctx: &mut C)
  where K: for<'a> From<&'a Path> {
    self.dequeue_fs_events(storage, ctx);
    self.reload_dirties(storage, ctx);
  }
}

/// Resource store. Responsible for holding and presenting resources.
pub struct Store<C, K> {
  storage: Storage<C, K>,
  synchronizer: Synchronizer<C, K>,
}

impl<C, K> Store<C, K>
where K: Key
{
  /// Create a new store.
  ///
  /// # Failures
  ///
  /// This function will fail if the root path in the [`StoreOpt`] doesn’t resolve to a correct
  /// canonicalized path.
  pub fn new(opt: StoreOpt<C, K>) -> Result<Self, StoreError<K>> {
    // canonicalize the root because some platforms won’t correctly report file changes otherwise
    let root = &opt.root;
    let canon_root = root
      .canonicalize()
      .map_err(|_| StoreError::RootDoesNotExist(root.to_owned()))?;

    // create the mpsc channel to communicate with the file watcher
    let (wsx, wrx) = channel();
    let mut watcher = notify::watcher(wsx, opt.debounce_duration).unwrap();

    // spawn a new thread in which we look for events
    let _ = watcher.watch(&canon_root, RecursiveMode::Recursive);

    // create the storage
    let storage = Storage::new(canon_root);

    // create the synchronizer
    let synchronizer = Synchronizer::new(watcher, wrx, opt.discovery);

    let store = Store {
      storage,
      synchronizer,
    };

    Ok(store)
  }

  /// Synchronize the [`Store`] by updating the resources that ought to with a provided context.
  pub fn sync(&mut self, ctx: &mut C)
  where K: for<'a> From<&'a Path> {
    self.synchronizer.sync(&mut self.storage, ctx);
  }
}

impl<C, K> Deref for Store<C, K> {
  type Target = Storage<C, K>;

  fn deref(&self) -> &Self::Target {
    &self.storage
  }
}

impl<C, K> DerefMut for Store<C, K> {
  fn deref_mut(&mut self) -> &mut Self::Target {
    &mut self.storage
  }
}

/// Various options to customize a [`Store`].
///
/// Feel free to inspect all of its declared methods for further information.
pub struct StoreOpt<C, K> {
  root: PathBuf,
  debounce_duration: Duration,
  discovery: Discovery<C, K>,
}

impl<C, K> Default for StoreOpt<C, K> {
  fn default() -> Self {
    StoreOpt {
      root: PathBuf::from("."),
      debounce_duration: Duration::from_millis(50),
      discovery: Discovery::default(),
    }
  }
}

impl<C, K> StoreOpt<C, K> {
  /// Change the debounce duration used to determine whether a resource should be
  /// reloaded or not.
  ///
  /// A [`Store`] will wait that amount of time before deciding an resource should be reloaded after
  /// it has changed on the filesystem. That is required in order to cope with write streaming, that
  /// generates a lot of write event.
  ///
  /// # Default
  ///
  /// Defaults to `50` milliseconds.
  #[inline]
  pub fn set_debounce_duration(self, duration: Duration) -> Self {
    StoreOpt {
      debounce_duration: duration,
      ..self
    }
  }

  /// Get the debounce duration.
  #[inline]
  pub fn debounce_duration(&self) -> Duration {
    self.debounce_duration
  }

  /// Change the root directory from which the [`Store`] will be watching file changes.
  ///
  /// # Default
  ///
  /// Defaults to `"."`.
  #[inline]
  pub fn set_root<P>(self, root: P) -> Self
  where P: AsRef<Path> {
    StoreOpt {
      root: root.as_ref().to_owned(),
      ..self
    }
  }

  /// Get root directory.
  #[inline]
  pub fn root(&self) -> &Path {
    &self.root
  }

  /// Change the discovery mechanism.
  ///
  /// # Default
  ///
  /// Defaults to `Discovery::default()`.
  #[inline]
  pub fn set_discovery(self, discovery: Discovery<C, K>) -> Self {
    StoreOpt { discovery, ..self }
  }

  /// Get the discovery mechanism.
  #[inline]
  pub fn discovery(&self) -> &Discovery<C, K> {
    &self.discovery
  }
}

/// Discovery.
///
/// Such an object is called whenever a new resource is discovered and is relied on to decide what
/// to do with the resource.
///
/// If you don’t care about discovering new resources, feel free to use the [`Default`] implementation.
pub struct Discovery<C, K> {
  closure: Box<dyn FnMut(&Path, &mut Storage<C, K>, &mut C)>,
}

impl<C, K> Discovery<C, K> {
  /// Create an new filter.
  ///
  /// The closure is passed the path of the discovered resource along with the storage and the
  /// context so that you can [`get`] that resource if you want. Keep in mind that the path is a raw
  /// and absolute path: you’ll have to extract the key (according to the type of resource you
  /// target) and pattern-match the extension / mime type on your own to choose which type of
  /// resource you want to get. Or you’ll just go full one-way and use the same resource type for
  /// all discovery, that’s also possible.
  ///
  /// [`get`]: crate::load::Storage::get
  pub fn new<F>(f: F) -> Self
  where F: 'static + FnMut(&Path, &mut Storage<C, K>, &mut C) {
    Discovery {
      closure: Box::new(f),
    }
  }

  /// Filter a discovery.
  fn discover(&mut self, path: &Path, storage: &mut Storage<C, K>, ctx: &mut C) {
    (self.closure)(path, storage, ctx)
  }
}

/// The default filter.
///
///   - Ignores any discovery.
impl<C, K> Default for Discovery<C, K> {
  fn default() -> Self {
    Discovery::new(|_, _, _| {})
  }
}
