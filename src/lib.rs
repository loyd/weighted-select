use std::marker::PhantomData;

use futures::{prelude::*, stream::Fuse};

#[cfg(test)]
mod tests;

/// An adapter for merging the output of several streams with priority.
///
/// The merged stream produces items from either of the underlying streams as they become available,
/// and the streams are polled according to priority.
#[must_use = "streams do nothing unless polled"]
#[derive(Debug)]
pub struct Select<N> {
    head: N,
    cursor: u64,
    limit: u64,
}

impl<N> Stream for Select<N>
where
    N: IncompleteSelect,
{
    type Item = N::Item;
    type Error = N::Error;

    fn poll(&mut self) -> Poll<Option<N::Item>, N::Error> {
        let (cnt, res) = match self.head.poll_chain(self.cursor) {
            (_, Ok(Async::NotReady)) | (_, Ok(Async::Ready(None))) if self.cursor > 0 => {
                self.head.poll_chain(0)
            }
            res => res,
        };

        self.cursor = cnt % self.limit;
        res
    }
}

#[derive(Debug)]
pub struct SelectPart<S, N> {
    stream: Fuse<S>,
    weight: u32,
    start_at: u64,
    prev_start_at: u64,
    next: N,
}

pub trait IncompleteSelect: Sized {
    type Item;
    type Error;

    fn append<NS>(self, stream: NS, weight: u32) -> SelectPart<NS, Self>
    where
        NS: Stream<Item = Self::Item, Error = Self::Error>;

    fn build(self) -> Select<Self>;

    fn poll_chain(&mut self, cursor: u64) -> (u64, Poll<Option<Self::Item>, Self::Error>);
}

impl<S, N> IncompleteSelect for SelectPart<S, N>
where
    S: Stream,
    N: IncompleteSelect<Item = S::Item, Error = S::Error>,
{
    type Item = S::Item;
    type Error = S::Error;

    fn append<NS>(self, stream: NS, weight: u32) -> SelectPart<NS, Self>
    where
        NS: Stream<Item = S::Item, Error = S::Error>,
    {
        assert!(weight > 0);

        let start_at = self.start_at + u64::from(self.weight);

        SelectPart {
            stream: stream.fuse(),
            weight,
            start_at,
            prev_start_at: start_at + u64::from(weight),
            next: self,
        }
    }

    fn build(self) -> Select<Self> {
        Select {
            limit: self.prev_start_at,
            head: self,
            cursor: 0,
        }
    }

    fn poll_chain(&mut self, cursor: u64) -> (u64, Poll<Option<Self::Item>, Self::Error>) {
        let (cursor, next_done) = if cursor < self.start_at {
            match self.next.poll_chain(cursor) {
                (cursor, Ok(Async::Ready(None))) => (cursor, true),
                (cursor, Ok(Async::NotReady)) => (cursor, false),
                result => return result,
            }
        } else {
            (cursor, cursor == 0)
        };

        debug_assert!(cursor >= self.start_at);

        match self.stream.poll() {
            Ok(Async::Ready(None)) if next_done => (self.prev_start_at, Ok(Async::Ready(None))),
            Ok(Async::NotReady) | Ok(Async::Ready(None)) => {
                (self.prev_start_at, Ok(Async::NotReady))
            }
            Err(err) => (self.prev_start_at, Err(err)),
            x => (cursor + 1, x),
        }
    }
}

#[derive(Debug)]
struct Terminal<I, E>(PhantomData<(I, E)>);

impl<I, E> IncompleteSelect for Terminal<I, E> {
    type Item = I;
    type Error = E;

    fn append<NS>(self, stream: NS, weight: u32) -> SelectPart<NS, Self>
    where
        NS: Stream<Item = I, Error = E>,
    {
        assert!(weight > 0);

        SelectPart {
            stream: stream.fuse(),
            weight,
            start_at: 0,
            prev_start_at: u64::from(weight),
            next: self,
        }
    }

    fn build(self) -> Select<Self> {
        Select {
            limit: 1, // Avoid calculating the remainder with a divisor of zero.
            head: self,
            cursor: 0,
        }
    }

    #[inline]
    fn poll_chain(&mut self, cursor: u64) -> (u64, Poll<Option<Self::Item>, Self::Error>) {
        debug_assert_eq!(cursor, 0);
        (0, Ok(Async::Ready(None)))
    }
}

pub fn new<I, E>() -> impl IncompleteSelect<Item = I, Error = E> {
    Terminal(PhantomData)
}
