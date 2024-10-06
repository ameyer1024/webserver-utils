
mod wrapper {
    use std::pin::Pin;

    pin_project_lite::pin_project! {
        pub struct UnsafeSendWrapper<T> {
            #[pin] inner: T,
        }
    }
    impl<T> UnsafeSendWrapper<T> {
        pub unsafe fn new(inner: T) -> Self {
            Self { inner }
        }
    }
    impl<T> std::future::Future for UnsafeSendWrapper<T> where T: std::future::Future {
        type Output = T::Output;
        fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
            let this = self.project();
            this.inner.poll(cx)
        }
    }
    unsafe impl<T> Send for UnsafeSendWrapper<T> {}
}
pub use wrapper::UnsafeSendWrapper;

#[must_use]
pub fn coro_await<'a, F, Out>(f: F) -> CoroAwaitFuture<'a, Out>
    where F: FnOnce(&CoroAwaiter<'_>) -> Out + 'a,
        Out: 'a,
{
    let coroutine = corosensei::ScopedCoroutine::<*mut std::task::Context<'static>, (), Out, corosensei::stack::DefaultStack>::new(move |yielder, res| {
        let awaiter = CoroAwaiter {
            yielder,
            context: std::cell::Cell::new(Some(res)),
        };
        f(&awaiter)
    });
    CoroAwaitFuture {
        coroutine,
    }
}

pub struct CoroAwaitFuture<'a, Out: 'a> {
    coroutine: corosensei::ScopedCoroutine::<'a, *mut std::task::Context<'static>, (), Out, corosensei::stack::DefaultStack>,
}

impl<'a, Out> std::future::Future for CoroAwaitFuture<'a, Out> {
    type Output = Out;
    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        use corosensei::CoroutineResult as CoResult;
        use std::task::{Poll, Context};

        match self.coroutine.resume(cx as *mut _ as *mut Context<'static>) {
            CoResult::Yield(_) => Poll::Pending,
            CoResult::Return(v) => return Poll::Ready(v),
        }
    }
}

pub struct CoroAwaiter<'a> {
    yielder: &'a corosensei::Yielder<*mut std::task::Context<'static>, ()>,
    context: std::cell::Cell<Option<*mut std::task::Context<'static>>>,
}

impl<'a> CoroAwaiter<'a> {
    pub fn block_on<O>(&self, mut f: impl std::future::Future<Output = O>) -> O {
        let mut pinned = std::pin::pin!(f);
        let mut context = self.context.take().unwrap();
        loop {
            let res = {
                // Safety: the context pointer is alive for the period between
                // the resume that returned the pointer and the next suspend
                // call, which gives up the context.
                //
                // While this method does not mutably borrow the CoroAwaiter,
                // it is not possible for a reentrant call in poll to cause a
                // suspend during the borrow of the context, as the context
                // is not available from the CoroAwaiter (using Cell::take),
                // and attempts at reentrant calls will panic.
                //
                // The 'static lifetime on the Context struct isn't great,
                // but there's no accurate lifetime that could replace it.
                let ctx = unsafe { &mut *context };

                pinned.as_mut().poll(ctx)

                // Borrow on context ends here.
            };
            match res {
                std::task::Poll::Ready(res) => {
                    self.context.set(Some(context));
                    return res;
                },
                std::task::Poll::Pending => {
                    // Borrow on context is not held across suspend point.
                    // The lifetime of the old context ends when this is called,
                    // and the lifetime of the new context begins when it returns.
                    context = self.yielder.suspend(());
                }
            }
        }
    }
}
