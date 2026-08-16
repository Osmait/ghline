//! Something that answers, eventually.
//!
//! Both programs put their slow work — a process, a network call, a lexer on
//! a large file — on a thread and talk to it by sending a request and picking
//! answers up later. The state layer's whole relationship with it is those
//! two verbs, and it held the concrete `Service` to get them.
//!
//! Behind a trait, three things follow. A test can hand in a worker that
//! answers immediately rather than starting a thread and hoping. The binary
//! decides there is one, which is where that decision belongs. And the state
//! layer stops naming a type from the layer below it.

/// The worker is not coming back.
///
/// Distinct from "nothing yet" on purpose: they look identical to a caller
/// that only asks whether an answer arrived, and one of them lasts for ever.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Gone;

/// Somewhere to send `Req` and collect `Res` later.
///
/// No thread bounds: the thing that holds one lives on the thread that draws,
/// and a `Receiver` is `Send` but not `Sync`. Requiring more than is used
/// would rule out the obvious implementation.
pub trait Worker<Req, Res> {
    /// Hands a request over, reporting whether it got there.
    ///
    /// The answer matters: a request dropped because the thread is gone would
    /// otherwise leave whatever asked for it marked as loading for ever, with
    /// a skeleton animating over data that is never coming.
    fn send(&self, req: Req) -> bool;

    /// The next answer, if one has arrived.
    ///
    /// `Gone` rather than `None` when the worker has died, because the two
    /// are indistinguishable to a caller and only one of them is temporary.
    fn poll(&self) -> Result<Option<Res>, Gone>;
}

/// A worker that answers on the spot, by calling `handle` on the thread that
/// asked.
///
/// For tests, and for anything that wants the protocol without the thread.
/// Answers queue up in order, so a test sends three requests and drains three
/// responses rather than racing a thread to find out.
pub struct Immediate<Req, Res> {
    handle: Box<dyn Fn(Req) -> Res>,
    answers: std::sync::Mutex<std::collections::VecDeque<Res>>,
}

impl<Req, Res> Immediate<Req, Res> {
    /// A worker that answers every request by calling `handle`.
    ///
    /// `handle` runs on the thread that called `send`, before it returns, so
    /// a test that sends and then polls is not racing anything.
    pub fn new(handle: impl Fn(Req) -> Res + 'static) -> Self {
        Self {
            handle: Box::new(handle),
            answers: std::sync::Mutex::new(std::collections::VecDeque::new()),
        }
    }
}

impl<Req, Res> Worker<Req, Res> for Immediate<Req, Res> {
    fn send(&self, req: Req) -> bool {
        let res = (self.handle)(req);
        match self.answers.lock() {
            Ok(mut q) => {
                q.push_back(res);
                true
            }
            // A poisoned lock means a previous handler panicked. Saying the
            // send failed is honest: nothing will come of it.
            Err(_) => false,
        }
    }

    fn poll(&self) -> Result<Option<Res>, Gone> {
        match self.answers.lock() {
            Ok(mut q) => Ok(q.pop_front()),
            Err(_) => Err(Gone),
        }
    }
}

/// A worker that is already dead, for testing what happens then.
pub struct Dead;

impl<Req, Res> Worker<Req, Res> for Dead {
    fn send(&self, _req: Req) -> bool {
        false
    }

    fn poll(&self) -> Result<Option<Res>, Gone> {
        Err(Gone)
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "assertions"
)]
mod tests {
    use super::*;

    #[test]
    fn an_immediate_worker_answers_in_the_order_it_was_asked() {
        let w = Immediate::new(|n: u32| n * 2);
        assert!(w.send(1));
        assert!(w.send(2));
        assert_eq!(w.poll(), Ok(Some(2)));
        assert_eq!(w.poll(), Ok(Some(4)));
        assert_eq!(w.poll(), Ok(None), "and then nothing, rather than gone");
    }

    #[test]
    fn a_dead_worker_refuses_and_says_it_is_gone() {
        // The two answers a caller has to tell apart: nothing yet, and never.
        let w: &dyn Worker<u32, u32> = &Dead;
        assert!(!w.send(1));
        assert_eq!(w.poll(), Err(Gone));
    }

    #[test]
    fn nothing_yet_and_never_are_different_answers() {
        let idle = Immediate::<u32, u32>::new(|n| n);
        assert_eq!(idle.poll(), Ok(None));
        let dead: &dyn Worker<u32, u32> = &Dead;
        assert_ne!(dead.poll(), Ok(None));
    }
}
