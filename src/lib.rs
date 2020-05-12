//! Hot-reloading, loadable and reloadable resources.
//!
//! # Foreword
//!
//! Resources are objects that live in a store and can be hot-reloaded – i.e. they can change
//! without you interacting with them. There are currently two types of resources supported:
//!
//!   - **Filesystem resources**, which are resources that live on the filesystem and have a real
//!     representation (i.e. a *file* for short).
//!   - **Logical resources**, which are resources that are computed and don’t directly require any
//!     I/O.
//!
//! Resources are referred to by *keys*. A *key* is a typed index that contains enough information
//! to uniquely identify a resource living in a store.
//!
//! This small introduction will give you enough information and examples to get your feet wet with
//! `warmy`. If you want to know more, feel free to visit the documentation of submodules.
//!
//! ## Feature-gates
//!
//! Here’s an exhaustive list of feature-gates available:
//!
//!   - `"arc"`: changes the internal representation of resources in order to use [`Arc`] and
//!     [`Mutex`], allowing for cross-thread sharing of resources. This is a current patch in the
//!     waiting of a better asynchronous solution.
//!   - `"json"`: provides a [`Json`] type that you can use as loading method to automatically load
//!     any type that implements [`serde::Deserialize`] and encoded as [JSON]. You don’t even have
//!     to implement [`Load`] by your own! **Enabled by default**
//!   - `"ron-impl"`: provides a [`Ron`] type that you can use as loading method to automatically
//!     load any type that implemetns [`serde::Deserialize`] and encoded as [RON].
//!   - `"toml-impl"`: provides a [`Toml`] type that you can use as loading method to automatically
//!     load any type that implements [`serde::Deserialize`] and encoded as [TOML].
//!
//! # Loading a resource
//!
//! *Loading* is the action of getting an object out of a given location. That location is often
//! your filesystem but it can also be a memory area – mapped files or memory parsing. In `warmy`,
//! loading is implemented *per-type*: this means you have to implement a trait on a type so that
//! any object of that type can be loaded. The trait to implement is [`Load`]. We’re interested in
//! four items:
//!
//!   - The [`Store`], which holds and caches resources.
//!   - The [`Key`] type variable, used to tell `warmy` which kind of resource your store knows how
//!     to represent and what information the key must contain.
//!   - The [`Load::Error`] associated type, that is the error type used when loading fails.
//!   - The [`Load::load`] method, which is the method called to load your resource in a given
//!     store.
//!
//! ## Store
//!
//! A [`Store`] is responsible for holding and caching resources. Each [`Store`] is associated with a
//! *root*, which is a path on the filesystem all filesystem resources will come from. You create a
//! [`Store`] by giving it a [`StoreOpt`], which is used to customize the [`Store`] – if you don’t
//! need it nor care about it for the moment, just use `Store::default`.
//!
//! ```rust
//! use warmy::{SimpleKey, Store, StoreOpt};
//!
//! let res = Store::<(), SimpleKey>::new(StoreOpt::default());
//!
//! match res {
//!   Err(e) => {
//!     eprintln!("unable to create the store: {}", e);
//!   }
//!
//!   Ok(store) => ()
//! }
//! ```
//!
//! As you can see, the [`Store`] has two type variables. These type variables refer to the types of
//! *context* you want to use with your resources and the type of keys. For now we’ll use `()` for
//! the context as we don’t want contexts – but more to come – and the common [`SimpleKey`] type
//! for keys. Keep on reading.
//!
//! ## The `Key` type variable
//!
//! The key type must implement [`Key`], which is the class of types recognized as keys by
//! `warmy`. In theory, you shouldn’t worry about that trait because `warmy` already ships with some
//! key types.
//!
//! > If you really want to implement [`Key`], have a look at its documentation for further details.
//!
//! Keys are a core concept in `warmy` as they are objects that uniquely represent resources –
//! should they be on a filesystem or in memory. You will refer to your resources with those keys.
//!
//! ### Special case: simple keys
//!
//! A *simple key* (a.k.a. [`SimpleKey`]) is a key used to express common situations in which you
//! might have resources from the filesystem and from logical locations. It’s provided for
//! convenience, so that you don’t have to write that type and implement [`Key`]. In most
//! situations, it should be enough for you – of course, if you need more details, feel free to
//! define your own key type.
//!
//! ## The `Load::Error` associated type
//!
//! This associated type must be set to the type of error your loading implementation might
//! generate. For instance, if you load something with [serde-json], you might want to set it to
//! °serde_json::Error`. This way of doing is very common in Rust; you shouldn’t feel uncomfortable
//! with it.
//!
//! > On a general note, you should always try to stick to precise and accurate errors types. Avoid
//! > simple types such as `String` or `u64` and prefer to use detailed, algebraic datatypes.
//!
//! ## The [`Load::load`] method
//!
//! This is the entry-point method. [`Load::load`] must be implemented in order for `warmy` to know
//! how to read the resource. Let’s implement it for two types: one that represents a resource on
//! the filesystem, one computed from memory.
//!
//! ```rust
//! use std::fmt;
//! use std::fs::File;
//! use std::io::{self, Read};
//! use warmy::{Load, Loaded, SimpleKey, Storage};
//!
//! // Possible errors that might happen.
//! #[derive(Debug)]
//! enum Error {
//!   CannotLoadFromFS,
//!   CannotLoadFromLogical,
//!   IOError(io::Error)
//! }
//!
//! impl fmt::Display for Error {
//!   fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
//!     match *self {
//!       Error::CannotLoadFromFS => f.write_str("cannot load from file system"),
//!       Error::CannotLoadFromLogical => f.write_str("cannot load from logical"),
//!       Error::IOError(ref e) => write!(f, "IO error: {}", e),
//!     }
//!   }
//! }
//!
//! // The resource we want to take from a file.
//! struct FromFS(String);
//!
//! // The resource we want to compute from memory.
//! struct FromMem(usize);
//!
//! impl<C> Load<C, SimpleKey> for FromFS {
//!   type Error = Error;
//!
//!   fn load(
//!     key: SimpleKey,
//!     storage: &mut Storage<C, SimpleKey>,
//!     _: &mut C
//!   ) -> Result<Loaded<Self, SimpleKey>, Self::Error> {
//!     // as we only accept filesystem here, we’ll ensure the key is a filesystem one
//!     match key {
//!       SimpleKey::Path(path) => {
//!         let mut fh = File::open(path).map_err(Error::IOError)?;
//!         let mut s = String::new();
//!         fh.read_to_string(&mut s);
//!
//!         Ok(FromFS(s).into())
//!       }
//!
//!       SimpleKey::Logical(_) => Err(Error::CannotLoadFromLogical)
//!     }
//!   }
//! }
//!
//! impl<C> Load<C, SimpleKey> for FromMem {
//!   type Error = Error;
//!
//!   fn load(
//!     key: SimpleKey,
//!     storage: &mut Storage<C, SimpleKey>,
//!     _: &mut C
//!   ) -> Result<Loaded<Self, SimpleKey>, Self::Error> {
//!     // ensure we only accept logical resources
//!     match key {
//!       SimpleKey::Logical(key) => {
//!         // this is a bit dummy, but why not?
//!         Ok(FromMem(key.len()).into())
//!       }
//!
//!       SimpleKey::Path(_) => Err(Error::CannotLoadFromFS)
//!     }
//!   }
//! }
//! ```
//!
//! As you can see here, there’re a few new concepts:
//!
//!   - [`Loaded`]: A type you have to wrap your object in to express dependencies. Because it
//!     implements `From<T> for Loaded<T>`, you can use `.into()` to state you don’t have any
//!     dependencies.
//!   - [`Storage`]: This is the minimal structure that holds and caches your resources. A [`Store`]
//!     is actually the *interface structure* you will handle in your client code.
//!
//! ## Express your dependencies with Loaded
//!
//! An object of type [`Loaded`] gives information to `warmy` about your dependencies. Upon loading –
//! i.e. your resource is successfully *loaded* – you can tell `warmy` which resources your loaded
//! resource depends on. This is a bit tricky, though, because a difference is important to make
//! there.
//!
//! When you implement [`Load::load`], you are handed a [`Storage`]. You can use that [`Storage`]
//! to load additional resources and gather them in your resources. When those additional resources
//! get reloaded, if you directly embed the resources in your object, you will automatically see the
//! automated resources – that is the whole point of this crate! However, if you don’t express a
//! *dependency relationship* to those resources, your former resource will not reload – it will
//! just use automatically-synced resources, but it will not reload itself. This is a bit touchy
//! but let’s take an example of a typical situation where you might want to use dependencies and
//! then dependency graphs:
//!
//!   1. You want to load an object that is represented by aggregation of several values /
//!      resources.
//!   2. You choose to use a *logical resource* and guess all the files to load from.
//!   3. When you implement [`Load::load`], you open several files, load them into memory, compose
//!      them and finally end up with your object.
//!   4. You return your object from [`Load::load`] with no dependencies (i.e. you use `.into()` on
//!      it).
//!
//! What is going to happen here is that if any file your resource depends on changes, since they
//! don’t have a proper resource in the store, your object will see nothing. A typical
//! solution there is to load those files as proper resources and put those keys in the returned
//! [`Loaded`] object to express that you *depend on the reloading of the objects referred by these
//! keys*. It’s a bit touchy but you will eventually find yourself in a situation when this
//! [`Loaded`] thing will help you. You will then use [`Loaded::with_deps`]. See the documentation of
//! [`Loaded`] for further information.
//!
//! > Fun fact: logical resources were introduced to solve that problem along with dependency
//! > graphs.
//!
//! ## Let’s get some things!
//!
//! When you have implemented [`Load`], you’re set and ready to get (cached) resources. You have
//! several functions to achieve that goal:
//!
//!   - [`Store::get`], used to get a resource. This will effectively load it if it’s the first time
//!     it’s asked. If it’s not, it will use a cached version.
//!   - [`Store::get_proxied`], a special version of [`Store::get`]. If the initial loading
//!     (non-cached) fails to load (missing resource, fail to parse, whatever), a *proxy* will be
//!     used – passed in to [`Store::get_proxied`]. This value is lazy though, so if the loading
//!     succeeds, that value won’t ever be evaluated.
//!
//! Let’s focus on [`Store::get`] for this tutorial.
//!
//! ```rust
//! use std::fmt;
//! use std::fs::File;
//! use std::io::{self, Read};
//! use std::path::Path;
//! use warmy::{Load, Loaded, SimpleKey, Store, StoreOpt, Storage};
//!
//! // Possible errors that might happen.
//! #[derive(Debug)]
//! enum Error {
//!   CannotLoadFromFS,
//!   CannotLoadFromLogical,
//!   IOError(io::Error)
//! }
//!
//! impl fmt::Display for Error {
//!   fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
//!     match *self {
//!       Error::CannotLoadFromFS => f.write_str("cannot load from file system"),
//!       Error::CannotLoadFromLogical => f.write_str("cannot load from logical"),
//!       Error::IOError(ref e) => write!(f, "IO error: {}", e),
//!     }
//!   }
//! }
//!
//! // The resource we want to take from a file.
//! struct FromFS(String);
//!
//! impl<C> Load<C, SimpleKey> for FromFS {
//!   type Error = Error;
//!
//!   fn load(
//!     key: SimpleKey,
//!     storage: &mut Storage<C, SimpleKey>,
//!     _: &mut C
//!   ) -> Result<Loaded<Self, SimpleKey>, Self::Error> {
//!     // as we only accept filesystem here, we’ll ensure the key is a filesystem one
//!     match key {
//!       SimpleKey::Path(path) => {
//!         let mut fh = File::open(path).map_err(Error::IOError)?;
//!         let mut s = String::new();
//!         fh.read_to_string(&mut s);
//!
//!         Ok(FromFS(s).into())
//!       }
//!
//!       SimpleKey::Logical(_) => Err(Error::CannotLoadFromLogical)
//!     }
//!   }
//! }
//!
//! fn main() {
//!   // we don’t need a context, so we’re using this mutable reference to unit
//!   let ctx = &mut ();
//!   let mut store: Store<(), SimpleKey> = Store::new(StoreOpt::default()).expect("store creation");
//!
//!   let my_resource = store.get::<FromFS>(&Path::new("/foo/bar/zoo.json").into(), ctx);
//!
//!   // …
//!
//!   // imagine that you’re in an event loop now and the resource has changed
//!   store.sync(ctx); // synchronize all resources (e.g. my_resource)
//! }
//! ```
//!
//! # Reloading a resource
//!
//! Most of the interesting concept of `warmy` is to enable you to hot-reload resources without
//! having to re-run your application. This is done via two items:
//!
//!   - [`Load::reload`], a method called whenever an object must be reloaded.
//!   - [`Store::sync`], a method to synchronize a [`Store`].
//!
//! The [`Load::reload`] function is very straight-forward: it’s called when the resource changes.
//! This situation happens:
//!
//!   - Either when the resource is on the filesystem (the file changes).
//!   - Or if it’s a dependent resource of one that has reloaded.
//!
//! See the documentation of [`Load::reload`] for further details.
//!
//! # Context inspection
//!
//! A context is a special value you can access to via a mutable reference when loading or
//! reloading. If you don’t need any, it’s highly recommended not to use `()` when implementing
//! `Load<C>` but leave it as type variable so that it compose better – i.e. `impl<C> Load<C>`.
//!
//! If you’re writing a library and need to have access to a specific value in a context, it’s also
//! recommended not to set the context type variable to the type of your context directly. If you do
//! that, no one will be able to use your library because types won’t match – or people will accept
//! to be restrained to your only types. A typical way to deal with that is by constraining a
//! type variable. The [`Inspect`] trait was introduced for this very purpose. For
//! instance:
//!
//! ```rust
//! use std::fmt;
//! use std::io;
//! use warmy::{Inspect, Load, Loaded, SimpleKey, Store, StoreOpt, Storage};
//!
//! // Possible errors that might happen.
//! #[derive(Debug)]
//! enum Error {
//!   CannotLoadFromFS,
//!   CannotLoadFromLogical,
//!   IOError(io::Error)
//! }
//!
//! impl fmt::Display for Error {
//!   fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
//!     match *self {
//!       Error::CannotLoadFromFS => f.write_str("cannot load from file system"),
//!       Error::CannotLoadFromLogical => f.write_str("cannot load from logical"),
//!       Error::IOError(ref e) => write!(f, "IO error: {}", e),
//!     }
//!   }
//! }
//!
//! struct Foo;
//!
//! struct Ctx {
//!   nb_res_loaded: usize
//! }
//!
//! impl<C> Load<C, SimpleKey> for Foo where Foo: for<'a> Inspect<'a, C, &'a mut Ctx> {
//!   type Error = Error;
//!
//!   fn load(
//!     key: SimpleKey,
//!     storage: &mut Storage<C, SimpleKey>,
//!     ctx: &mut C
//!   ) -> Result<Loaded<Self, SimpleKey>, Self::Error> {
//!     Self::inspect(ctx).nb_res_loaded += 1; // magic happens here!
//!
//!     Ok(Foo.into())
//!   }
//! }
//!
//! fn main() {
//!   use warmy::{Res, Store, StoreOpt};
//!
//!   let mut store: Store<Ctx, SimpleKey> = Store::new(StoreOpt::default()).unwrap();
//!   let mut ctx = Ctx { nb_res_loaded: 0 };
//!
//!   let r: Res<Foo> = store.get(&"test-0".into(), &mut ctx).unwrap();
//! }
//! ```
//!
//! In this example, because the context value we want is the same as the [`Store`]’s context, a
//! universal implementor of [`Inspect`] enables you to directly [`inspect`] the context. However,
//! if you wanted to inspect it more precisely, like with `&mut usize`, you would need to write an
//! implementation of [`Inspect`] for your types:
//!
//! ```rust
//! use std::fmt;
//! use std::io;
//! use warmy::{Inspect, Load, Loaded, SimpleKey, Store, StoreOpt, Storage};
//!
//! // Possible errors that might happen.
//! #[derive(Debug)]
//! enum Error {
//!   CannotLoadFromFS,
//!   CannotLoadFromLogical,
//!   IOError(io::Error)
//! }
//!
//! impl fmt::Display for Error {
//!   fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
//!     match *self {
//!       Error::CannotLoadFromFS => f.write_str("cannot load from file system"),
//!       Error::CannotLoadFromLogical => f.write_str("cannot load from logical"),
//!       Error::IOError(ref e) => write!(f, "IO error: {}", e),
//!     }
//!   }
//! }
//!
//! struct Foo;
//!
//! struct Ctx {
//!   nb_res_loaded: usize
//! }
//!
//! // this implementor states how the inspection should occur for Foo when the context has type
//! // Ctx: by targetting a mutable reference on a usize (i.e. the counter)
//! impl<'a> Inspect<'a, Ctx, &'a mut usize> for Foo {
//!   fn inspect(ctx: &mut Ctx) -> &mut usize {
//!     &mut ctx.nb_res_loaded
//!   }
//! }
//!
//! // notice the usize instead of Ctx here
//! impl<C> Load<C, SimpleKey> for Foo where Foo: for<'a> Inspect<'a, C, &'a mut usize> {
//!   type Error = Error;
//!
//!   fn load(
//!     key: SimpleKey,
//!     storage: &mut Storage<C, SimpleKey>,
//!     ctx: &mut C
//!   ) -> Result<Loaded<Self, SimpleKey>, Self::Error> {
//!     *Self::inspect(ctx) += 1; // direct access to the counter
//!
//!     Ok(Foo.into())
//!   }
//! }
//! ```
//!
//! # Load methods
//!
//! `warmy` supports load methods. Those are used to specify several ways to load an object of a
//! given type. By default, [`Load`] is implemented with the *default method* – which is `()`. If
//! you want more methods, you can set the type parameter to something else when implementing
//! [`Load`].
//!
//! You can also find several *methods* centralized in here, but you definitely don’t have to use
//! them.
//!
//! ## Universal JSON support
//!
//! The crate supports *universal JSON implementation*. You can use it via the
//! [`Json`] type.
//!
//! > Universal JSON support is feature-gated with `"json"`.
//!
//! Universal JSON can help and make your life and implementations easier. Basically, it means that
//! any type that implements [`serde::Deserialize`] can be loaded and hot-reloaded by `warmy`
//! with zero boilerplate from your side, just by asking `warmy` to get the given scarse resource.
//! This is done with the [`Store::get_by`] or [`Store::get_proxied_by`] methods.
//!
//! ```rust
//! use serde::Deserialize;
//! use warmy::{Res, SimpleKey, Store, StoreOpt};
//! use warmy::json::Json;
//! use std::thread::sleep;
//! use std::time::Duration;
//!
//! #[derive(Debug, Deserialize)]
//! #[serde(rename_all = "snake_case")]
//! struct Dog {
//!   name: String,
//!   gender: Gender
//! }
//!
//! impl Default for Dog {
//!   fn default() -> Self {
//!     Dog {
//!       name: "Norbert".to_owned(),
//!       gender: Gender::Male
//!     }
//!   }
//! }
//!
//! #[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
//! #[serde(rename_all = "snake_case")]
//! enum Gender {
//!   Female,
//!   Male
//! }
//!
//! fn main() {
//!   let mut store: Store<(), SimpleKey> = Store::new(StoreOpt::default()).unwrap();
//!   let ctx = &mut ();
//!
//!   let resource: Result<Res<Dog>, _> = store.get_by(&SimpleKey::from_path("/dog.json"), ctx, Json);
//!
//!   match resource {
//!     Ok(dog) => {
//!       loop {
//!         store.sync(ctx);
//!
//!         println!("Dog is {} and is a {:?}", dog.borrow().name, dog.borrow().gender);
//!         sleep(Duration::from_millis(1000));
//!       }
//!     }
//!
//!     Err(e) => eprintln!("{}", e)
//!   }
//! }
//! ```
//!
//! ## Universal TOML support
//!
//! The crate also supports *universal TOML implementation*. That implementation is available via
//! the [`Toml`] type.
//!
//! > Universal TOML support is feature-gated with `"toml-impl"`.
//!
//! The working mechanism is the same as with [universal JSON support](#universal-json-support).
//!
//! # Resource discovery
//!
//! Resource discovery is available via a simple mechanism: every time a new resource is available
//! on the filesystem, a closure of your choice is called. This closure is passed the [`Storage`]
//! of your [`Store`] along with its associated context, enabling you to insert new resources on
//! the fly.
//!
//! This is a bit different than the first option: this enables you to populate the store with
//! resources you don’t know yet – e.g. a texture is saved in the store’s root and gets
//! automatically added and reacted to.
//!
//! The feature is available via the [`StoreOpt`] object you have to create prior to generating a
//! new [`Store`]. See the [`StoreOpt::set_discovery`] and [`StoreOpt::discovery`] functions for
//! further details on how to use the resource discovery mechanism.
//!
//! [serde-json]: https://crates.io/crates/serde_json
//! [serde_json::Error]: https://docs.serde.rs/serde_json/struct.Error.html
//! [VFS]: https://en.wikipedia.org/wiki/Virtual_file_system
//! [`Key`]: crate::load::Key
//! [`Load`]: crate::load::Load
//! [`Load::Error`]: crate::load::Load::Error
//! [`Load::load`]: crate::load::Load::load
//! [`Load::reload`]: crate::load::Load::reload
//! [`Loaded`]: crate::load::Loaded
//! [`Loaded::with_deps`]: crate::load::Loaded::with_deps
//! [`Json`]: crate::json::Json
//! [`Toml`]: crate::toml::Toml
//! [`Ron`]: crate::ron::Ron
//! [`Storage`]: crate::load::Storage
//! [`Store`]: crate::load::Store
//! [`Store::get`]: crate::load::Storage::get
//! [`Store::get_by`]: crate::load::Storage::get_by
//! [`Store::get_proxied`]: crate::load::Storage::get_proxied
//! [`Store::get_proxied_by`]: crate::load::Storage::get_proxied_by
//! [`Store::sync`]: crate::load::Store::sync
//! [`StoreOpt`]: crate::load::StoreOpt
//! [`StoreOpt::set_discovery`]: crate::load::StoreOpt::set_discovery
//! [`StoreOpt::discovery`]: crate::load::StoreOpt::discovery
//! [`SimpleKey`]: crate::key::SimpleKey
//! [`Inspect`]: crate::context::Inspect
//! [`inspect`]: crate::context::Inspect::inspect
//! [`serde::Deserialize`]: https://docs.rs/serde/1.0.85/serde/trait.Deserialize.html
//! [`Arc`]: std::sync::Arc
//! [`Mutex`]: std::sync::Mutex
//! [JSON]: https://www.json.org
//! [TOML]: https://github.com/toml-lang/toml
//! [RON]: https://github.com/ron-rs/ron

pub mod context;
#[cfg(feature = "json")]
pub mod json;
pub mod key;
pub mod load;
pub mod res;
#[cfg(feature = "ron-impl")]
pub mod ron;
#[cfg(feature = "toml-impl")]
pub mod toml;

pub use crate::context::Inspect;
pub use crate::key::{Key, SimpleKey};
pub use crate::load::{
  Discovery, Load, Loaded, Storage, Store, StoreError, StoreErrorOr, StoreOpt,
};
pub use crate::res::Res;
