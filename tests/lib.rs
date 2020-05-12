use std::fmt;
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use tempfile::Builder;
use warmy::{Inspect, Load, Loaded, Res, SimpleKey, Storage, Store};

fn with_tmp_dir<F, B>(f: F)
where F: Fn(&Path) -> B {
  let tmp_dir = Builder::new()
    .prefix("warmy")
    .tempdir()
    .expect("create temporary directory");
  let _ = f(tmp_dir.path());
  tmp_dir.close().expect("close the temporary directory");
}

fn with_store<F, B, C>(f: F)
where F: Fn(Store<C, SimpleKey>) -> B {
  with_tmp_dir(|tmp_dir| {
    let opt = warmy::StoreOpt::default().set_root(tmp_dir.to_owned());

    let store = warmy::Store::new(opt).expect("create store");
    f(store)
  })
}

/// Timeout in milliseconds to wait before determining that there’s something wrong with notify.
const QUEUE_TIMEOUT_MS: u64 = 5000; // 5s

#[derive(Debug, Eq, PartialEq)]
struct Foo(String);

#[derive(Debug, Eq, PartialEq)]
enum TestErr {
  WrongKey(SimpleKey),
}

impl fmt::Display for TestErr {
  fn fmt(&self, f: &mut fmt::Formatter) -> Result<(), fmt::Error> {
    match *self {
      TestErr::WrongKey(ref key) => write!(f, "wrong key: {}", key),
    }
  }
}

impl<C> Load<C, SimpleKey> for Foo {
  type Error = TestErr;

  fn load(
    key: SimpleKey,
    _: &mut Storage<C, SimpleKey>,
    _: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    if let SimpleKey::Path(ref key) = key {
      let mut s = String::new();

      {
        let path = key.as_path();
        eprintln!("KEY: {}", path.display());
        let mut fh = File::open(path).unwrap();
        let _ = fh.read_to_string(&mut s);
      }

      let foo = Foo(s);

      Ok(foo.into())
    } else {
      Err(TestErr::WrongKey(key))
    }
  }
}

// This struct has a Load implementation that is const: it doesn’t load anything from the file.
struct Stupid;

// a tricky version that doesn’t actually read the file but return something constant… it’s stupid,
// but it’s there to test methods
impl<C> Load<C, SimpleKey, Stupid> for Foo {
  type Error = TestErr;

  fn load(
    _: SimpleKey,
    _: &mut Storage<C, SimpleKey>,
    _: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    eprintln!("hello");
    let foo = Foo("stupid".to_owned());
    Ok(foo.into())
  }
}

#[derive(Debug, Eq, PartialEq)]
struct Bar(String);

impl<C> Load<C, SimpleKey> for Bar {
  type Error = TestErr;

  fn load(
    _: SimpleKey,
    _: &mut Storage<C, SimpleKey>,
    _: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    let bar = Bar("bar".to_owned());
    Ok(bar.into())
  }
}

#[derive(Debug, Eq, PartialEq)]
struct Zoo(String);

impl<C> Load<C, SimpleKey> for Zoo {
  type Error = TestErr;

  fn load(
    key: SimpleKey,
    _: &mut Storage<C, SimpleKey>,
    _: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    if let SimpleKey::Logical(key) = key {
      let content = key.as_str().to_owned();
      let zoo = Zoo(content);

      Ok(zoo.into())
    } else {
      Err(TestErr::WrongKey(key))
    }
  }
}

#[derive(Debug, Eq, PartialEq)]
struct LogicalFoo(String);

impl<C> Load<C, SimpleKey> for LogicalFoo {
  type Error = TestErr;

  fn load(
    key: SimpleKey,
    storage: &mut Storage<C, SimpleKey>,
    ctx: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    if let SimpleKey::Logical(key) = key {
      let fs_key = Path::new(&key).into();
      let foo: Res<Foo> = storage.get(&fs_key, ctx).unwrap();

      let content = foo.borrow().0.clone();
      let zoo = LogicalFoo(content);

      let r = Loaded::with_deps(zoo, vec![fs_key]);
      Ok(r)
    } else {
      Err(TestErr::WrongKey(key))
    }
  }
}

#[test]
fn create_store() {
  with_store(|_: Store<(), SimpleKey>| {})
}

#[test]
fn witness_sync() {
  with_store(|mut store| {
    let ctx = &mut ();
    let expected1 = "Hello, world!".to_owned();
    let expected2 = "Bye!".to_owned();

    let key = Path::new("foo.txt").into();
    let path = store.root().join("foo.txt");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected1.as_bytes());
    }

    let r: Res<Foo> = store
      .get(&key, ctx)
      .expect("object should be present at the given key");

    assert_eq!(r.borrow().0, expected1);

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected2.as_bytes());
    }

    let start_time = ::std::time::Instant::now();
    loop {
      store.sync(ctx);

      if r.borrow().0.as_str() == expected2.as_str() {
        break;
      }

      if start_time.elapsed() >= ::std::time::Duration::from_millis(QUEUE_TIMEOUT_MS) {
        panic!(
          "more than {} milliseconds were spent waiting for a filesystem event",
          QUEUE_TIMEOUT_MS
        );
      }
    }
  })
}

#[test]
fn vfs_leading_slash() {
  with_store(|mut store| {
    let ctx = &mut ();
    let expected1 = "Hello, world!".to_owned();
    let expected2 = "Bye!".to_owned();

    let key = Path::new("/foo.txt").into();
    let path = store.root().join("foo.txt");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected1.as_bytes());
    }

    let r: Res<Foo> = store
      .get(&key, ctx)
      .expect("object should be present at the given key");

    assert_eq!(r.borrow().0, expected1);

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected2.as_bytes());
    }

    let start_time = ::std::time::Instant::now();
    loop {
      store.sync(ctx);

      if r.borrow().0.as_str() == expected2.as_str() {
        break;
      }

      if start_time.elapsed() >= ::std::time::Duration::from_millis(QUEUE_TIMEOUT_MS) {
        panic!(
          "more than {} milliseconds were spent waiting for a filesystem event",
          QUEUE_TIMEOUT_MS
        );
      }
    }
  })
}

#[test]
fn two_same_paths_diff_types() {
  with_store(|mut store| {
    let ctx = &mut ();
    let foo_key: SimpleKey = Path::new("a.txt").into();
    let bar_key = foo_key.clone();
    let path = store.root().join("a.txt");

    // create a.txt
    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(&b"foobarzoo"[..]);
    }

    let foo: Res<Foo> = store.get(&foo_key, ctx).unwrap();
    assert_eq!(foo.borrow().0.as_str(), "foobarzoo");

    let bar: Result<Res<Bar>, _> = store.get(&bar_key, ctx);
    assert!(bar.is_err());
  })
}

#[test]
fn logical_resource() {
  with_store(|mut store| {
    let key = "mem/uid/32197".into();
    let zoo: Res<Zoo> = store.get(&key, &mut ()).unwrap();
    assert_eq!(zoo.borrow().0.as_str(), "mem/uid/32197");
  })
}

#[test]
fn logical_with_deps() {
  with_store(|mut store| {
    let ctx = &mut ();
    let expected1 = "Hello, world!".to_owned();
    let expected2 = "Bye!".to_owned();

    let foo_key = Path::new("foo.txt").into();
    let path = store.root().join("foo.txt");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected1.as_bytes());
    }

    let _: Res<Foo> = store
      .get(&foo_key, ctx)
      .expect("object should be present at the given key");

    let log_foo_key = "foo.txt".into();
    let log_foo: Res<LogicalFoo> = store.get(&log_foo_key, ctx).unwrap();

    assert_eq!(log_foo.borrow().0.as_str(), "Hello, world!");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected2.as_bytes());
    }

    let start_time = ::std::time::Instant::now();
    loop {
      store.sync(ctx);

      if log_foo.borrow().0.as_str() == expected2.as_str() {
        break;
      }

      if start_time.elapsed() >= ::std::time::Duration::from_millis(QUEUE_TIMEOUT_MS) {
        eprintln!("log_foo = {:?}", log_foo.borrow().0.as_str());
        panic!(
          "more than {} milliseconds were spent waiting for a filesystem event",
          QUEUE_TIMEOUT_MS
        );
      }
    }
  })
}

#[derive(Debug, Eq, PartialEq)]
struct Ctx {
  foo_nb: u32,
  pew_nb: u32,
}

impl Ctx {
  fn new() -> Self {
    Ctx {
      foo_nb: 0,
      pew_nb: 0,
    }
  }
}

#[derive(Debug, Eq, PartialEq)]
struct FooWithCtx(String);

impl<'a> Inspect<'a, Ctx, &'a mut u32> for FooWithCtx {
  fn inspect(ctx: &mut Ctx) -> &mut u32 {
    &mut ctx.foo_nb
  }
}

impl<C> Load<C, SimpleKey> for FooWithCtx
where Self: for<'a> Inspect<'a, C, &'a mut u32>
{
  type Error = TestErr;

  fn load(
    key: SimpleKey,
    storage: &mut Storage<C, SimpleKey>,
    ctx: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    // load as if it was a Foo
    let Loaded { res, deps } = <Foo as Load<_, _, ()>>::load(key, storage, ctx)?;

    // increment the counter
    *Self::inspect(ctx) += 1;

    let r = Loaded::with_deps(FooWithCtx(res.0), deps);
    Ok(r)
  }
}

#[derive(Debug, Eq, PartialEq)]
struct Pew;

impl<'a> Inspect<'a, Ctx, &'a mut u32> for Pew {
  fn inspect(ctx: &mut Ctx) -> &mut u32 {
    &mut ctx.pew_nb
  }
}

impl<C> Load<C, SimpleKey> for Pew
where
  Self: for<'a> Inspect<'a, C, &'a mut u32>,
  FooWithCtx: for<'a> Inspect<'a, C, &'a mut u32>,
{
  type Error = TestErr;

  fn load(
    _: SimpleKey,
    _: &mut Storage<C, SimpleKey>,
    ctx: &mut C,
  ) -> Result<Loaded<Self, SimpleKey>, Self::Error>
  {
    // for the sake of the teste, just tap another resource as well
    *FooWithCtx::inspect(ctx) += 1;

    *Self::inspect(ctx) += 1;

    Ok(Pew.into())
  }
}

#[test]
fn foo_with_ctx() {
  with_store(|mut store| {
    let mut ctx = Ctx::new();

    let expected1 = "Hello, world!".to_owned();
    let expected2 = "Bye!".to_owned();

    let key = Path::new("foo.txt").into();
    let path = store.root().join("foo.txt");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected1.as_bytes());
    }

    let r: Res<FooWithCtx> = store
      .get(&key, &mut ctx)
      .expect("object should be present at the given key");

    assert_eq!(r.borrow().0, expected1);

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(expected2.as_bytes());
    }

    let start_time = ::std::time::Instant::now();
    loop {
      store.sync(&mut ctx);

      if r.borrow().0.as_str() == expected2.as_str() {
        break;
      }

      if start_time.elapsed() >= ::std::time::Duration::from_millis(QUEUE_TIMEOUT_MS) {
        panic!(
          "more than {} milliseconds were spent waiting for a filesystem event",
          QUEUE_TIMEOUT_MS
        );
      }
    }

    assert_eq!(ctx.foo_nb, 2);
  })
}

#[test]
fn foo_by_stupid() {
  with_store(|mut store| {
    let ctx = &mut ();
    let expected = "stupid";

    let key = Path::new("foo.txt").into();
    let path = store.root().join("foo.txt");

    {
      let mut fh = File::create(&path).unwrap();
      let _ = fh.write_all(&b"Hello, world!"[..]);
    }

    let r: Res<Foo> = store
      .get_by(&key, ctx, Stupid)
      .expect("object should be present at the given key");

    assert_eq!(&r.borrow().0, expected);
  })
}

#[test]
fn load_two_ctx() {
  with_store(|mut store| {
    let mut ctx = Ctx::new();

    let key = "pew".into();

    let _: Res<Pew> = store.get(&key, &mut ctx).expect("should always get a Pew");

    assert_eq!(ctx.foo_nb, 1);
    assert_eq!(ctx.pew_nb, 1);
  })
}
