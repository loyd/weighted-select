use futures::{prelude::*, stream::iter_ok, task};
use quickcheck_macros::quickcheck;

use crate::IncompleteSelect;

struct WithBreaks<S> {
    stream: S,
    flag: bool,
}

impl<S> WithBreaks<S> {
    fn new(stream: S) -> WithBreaks<S> {
        WithBreaks {
            stream,
            flag: false,
        }
    }
}

impl<S: Stream> Stream for WithBreaks<S> {
    type Item = S::Item;
    type Error = S::Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        self.flag = !self.flag;

        if self.flag {
            self.stream.poll()
        } else {
            task::current().notify();
            Ok(Async::NotReady)
        }
    }
}

#[test]
fn empty_select() {
    let select = crate::new::<(), ()>().build();

    let mut stream = select.wait();

    assert_eq!(stream.next(), None);
    assert_eq!(stream.next(), None);
}

#[test]
fn only_one_part() {
    let select = crate::new()
        .append(iter_ok::<_, ()>(vec![1u32, 2]), 1)
        .build();

    let mut stream = select.wait();

    assert_eq!(stream.next(), Some(Ok(1)));
    assert_eq!(stream.next(), Some(Ok(2)));
    assert_eq!(stream.next(), None);
}

#[test]
fn three_parts() {
    let select = crate::new()
        .append(iter_ok::<_, ()>(vec![1u32, 1]), 1)
        .append(iter_ok(vec![2, 2, 2]), 3)
        .append(iter_ok(vec![3, 3, 3, 3]), 1)
        .build();

    let actual = select.wait().collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(actual, vec![1, 2, 2, 2, 3, 1, 3, 3, 3]);
}

#[test]
fn incremental_build() {
    fn append(
        builder: impl IncompleteSelect<Item = u32, Error = ()>,
        data: Vec<u32>,
        weight: u32,
    ) -> impl IncompleteSelect<Item = u32, Error = ()> {
        builder.append(iter_ok(data), weight)
    }

    let select = crate::new();
    let select = append(select, vec![1u32, 1], 1);
    let select = append(select, vec![2, 2, 2], 3);
    let select = append(select, vec![3, 3, 3, 3], 1);
    let select = select.build();

    let actual = select.wait().collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(actual, vec![1, 2, 2, 2, 3, 1, 3, 3, 3]);
}

#[test]
fn three_parts_with_breaks() {
    let select = crate::new()
        .append(WithBreaks::new(iter_ok::<_, ()>(vec![1u32, 1])), 1)
        .append(WithBreaks::new(iter_ok(vec![2, 2, 2])), 3)
        .append(WithBreaks::new(iter_ok(vec![3, 3, 3, 3])), 1)
        .build();

    let actual = select.wait().collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(actual, vec![1, 2, 3, 2, 1, 2, 3, 3, 3]);

    let select = crate::new()
        .append(iter_ok::<_, ()>(vec![1u32, 1]), 1)
        .append(WithBreaks::new(iter_ok(vec![2, 2, 2])), 3)
        .append(iter_ok(vec![3, 3, 3, 3]), 1)
        .build();

    let actual = select.wait().collect::<Result<Vec<_>, _>>().unwrap();

    assert_eq!(actual, vec![1, 2, 3, 1, 2, 3, 2, 3, 3]);
}

#[quickcheck]
fn distribution(mut an: u16, aw: u8, mut bn: u16, bw: u8, mut cn: u16, cw: u8) {
    if aw == 0 || bw == 0 || cw == 0 {
        return;
    }

    let select = crate::new()
        .append(iter_ok::<_, ()>(vec![b'a'; an as usize]), u32::from(aw))
        .append(iter_ok(vec![b'b'; bn as usize]), u32::from(bw))
        .append(iter_ok(vec![b'c'; cn as usize]), u32::from(cw))
        .build();

    let aw = u16::from(aw);
    let bw = u16::from(bw);
    let cw = u16::from(cw);

    let actual = select.wait().collect::<Result<Vec<_>, _>>().unwrap();
    let mut expected = Vec::with_capacity((an + bn + cn) as usize);

    while an > 0 || bn > 0 || cn > 0 {
        for _ in 0..aw.min(an) {
            expected.push(b'a');
        }

        an = an.saturating_sub(aw);

        for _ in 0..bw.min(bn) {
            expected.push(b'b');
        }

        bn = bn.saturating_sub(bw);

        for _ in 0..cw.min(cn) {
            expected.push(b'c');
        }

        cn = cn.saturating_sub(cw);
    }

    assert_eq!(actual, expected);
}
