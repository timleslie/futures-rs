use core::pin::Pin;
use futures_core::future::TryFuture;
use futures_core::task::{self, Poll};

#[must_use = "futures do nothing unless polled"]
#[derive(Debug)]
pub(crate) enum TryChain<Fut1, Fut2, Data> {
    First(Fut1, Option<Data>),
    Second(Fut2),
    Empty,
}

pub(crate) enum TryChainAction<Fut2>
    where Fut2: TryFuture,
{
    Future(Fut2),
    Output(Result<Fut2::Ok, Fut2::Error>),
}

impl<Fut1, Fut2, Data> TryChain<Fut1, Fut2, Data>
    where Fut1: TryFuture,
          Fut2: TryFuture,
{
    pub(crate) fn new(fut1: Fut1, data: Data) -> TryChain<Fut1, Fut2, Data> {
        TryChain::First(fut1, Some(data))
    }

    pub(crate) fn poll<F>(
        self: Pin<&mut Self>,
        lw: &LocalWaker,
        f: F,
    ) -> Poll<Result<Fut2::Ok, Fut2::Error>>
        where F: FnOnce(Result<Fut1::Ok, Fut1::Error>, Data) -> TryChainAction<Fut2>,
    {
        let mut f = Some(f);

        // Safe to call `get_mut_unchecked` because we won't move the futures.
        let this = unsafe { Pin::get_mut_unchecked(self) };

        loop {
            let (output, data) = match this {
                TryChain::First(fut1, data) => {
                    // Poll the first future
                    match unsafe { Pin::new_unchecked(fut1) }.try_poll(lw) {
                        Poll::Pending => return Poll::Pending,
                        Poll::Ready(output) => (output, data.take().unwrap()),
                    }
                }
                TryChain::Second(fut2) => {
                    // Poll the second future
                    return unsafe { Pin::new_unchecked(fut2) }.try_poll(lw)
                }
                TryChain::Empty => {
                    panic!("future must not be polled after it returned `Poll::Ready`");
                }
            };

            *this = TryChain::Empty; // Drop fut1
            let f = f.take().unwrap();
            match f(output, data) {
                TryChainAction::Future(fut2) => *this = TryChain::Second(fut2),
                TryChainAction::Output(output) => return Poll::Ready(output),
            }
        }
    }
}