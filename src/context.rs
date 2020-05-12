//! Context related functions and types.

/// The inspect context trait.
///
/// This trait is very similar to the [`BorrowMut`] trait but is specific to the need of this crate
/// and how resources inspect contexts. In order to get why such a trait is needed, you must be
/// introduced to the semantics borrowing problem.
///
/// # The semantics borrowing problem
///
/// So imagine we have a `Foo` type. We would like to implement `Load` for `Foo` and use a `u32`
/// as mutable context as counter to increment every time a `Foo` gets loaded. However, imagine
/// that this is a library type. It would be a pity to stick to a `Storage<u32>`, because then we
/// couldn’t load our `Foo` with a more complex context type (for instance, provided by a binary’s
/// code). The typical trick to fix that problem is to use a polymorphic `Storage<C>` and instead
/// use something like `C: BorrowMut<u32>` as context type. That enables us to use any type of
/// context and still access the variable we need for counting foos. Neat.
///
/// However, consider another type, `Bar`, that also needs a `u32` to be incremented every time a
/// `Bar` resource gets loaded. It’s obvious that you cannot use the same `u32`. So there are a few
/// possibilities here:
///
///   - Use type wrappers to encode `FooCounter` and `BarCounter` but this is just additional type
///     safety and is orthogonal to our design.
///   - Use two distinct `u32` and wrap them in a `(u32, u32)`, at least.
///
/// The second option is the right way to go, but we now have a problem. As you can see,
/// `BorrowMut<u32> for (u32, u32)` is ambiguous (which `u32` pick?) and doesn’t allow you to pick one
/// and the other.
///
/// Even though the borrowed types are the same, the two possible implementations have different
/// semantics (one targets a counter for `Foo`, the other a counter for `Bar`). The [`Inspect`]
/// trait encodes this situation as a tuple of types:
///
///   - The borrower type — which is the same as `Self` in the `BorrowMut` trait.
///   - The inspected type – which is the same one as in the `BorrowMut` trait if you inspect as a
///     mutable reference to something.
///   - The inspector type – which doesn’t exist in the `BorrowMut` trait.
///   - The method type – i.e. doesn’t exist in `BorrowMut` and only serves to have different kind
///     of inspection regarding the method you use.
///
/// The inspector type gives the missing semantics to the borrow to *decide* how the data should be
/// inspected. The example above can then be rewritten correctly with the following:
///
/// ```
/// use warmy::Inspect;
///
/// struct Foo;
/// struct Bar;
///
/// struct Context {
///   foos: u32,
///   bars: u32
/// }
///
/// impl<'a> Inspect<'a, Context, &'a mut u32> for Foo {
///   fn inspect(ctx: &mut Context) -> &mut u32 {
///     &mut ctx.foos
///   }
/// }
///
/// impl<'a> Inspect<'a, Context, &'a mut u32> for Bar {
///   fn inspect(ctx: &mut Context) -> &mut u32 {
///     &mut ctx.bars
///   }
/// }
/// ```
///
/// And here you have it: borrowing two different objects with the same type from the same object,
/// something impossible with the standard `BorrowMut` trait.
///
/// # Universal implementors
///
/// Some implementations are provided by default so that you don’t have to write them.
///
/// First, if you target no context (i.e. `()`), the implementation is already there for you.
///
/// Then, two borrowing flavours are provided by default for you:
///
///   - Immutable full-context: you want to immutably borrow the whole context.
///   - Mutable full-context: you want to mutably borrow the whole context.
///
/// Those two might be useful in end libraries or binaries.
///
/// # A note on the lifetime
///
/// Because of being generic over the borrow lifetime, you can return any kind of borrow (not only
/// references). This is a huge advancement over the current `BorrowMut` trait as it’s still
/// possible to encode mutable references with `&'a mut _` but you can also returns any kind of
/// type, even with a lifetime outliving the borrow. This enables returning `()` or other exotic
/// kind of data (for instance, you might want to copy / clone something and not use any reference).
///
/// [`BorrowMut`]: std::borrow::BorrowMut
pub trait Inspect<'a, Ctx, Inspected, Method = ()> {
  /// Inspect the context.
  fn inspect(ctx: &'a mut Ctx) -> Inspected;
}

/// No-context universal implementor.
impl<'a, T, C, M> Inspect<'a, C, (), M> for T {
  fn inspect(_: &'a mut C) {}
}

/// Immutable full-context universal implementator.
impl<'a, T, C, M> Inspect<'a, C, &'a C, M> for T {
  fn inspect(ctx: &'a mut C) -> &'a C {
    ctx
  }
}

/// Mutable full-context universal implementator.
impl<'a, T, C, M> Inspect<'a, C, &'a mut C, M> for T {
  fn inspect(ctx: &'a mut C) -> &'a mut C {
    ctx
  }
}
