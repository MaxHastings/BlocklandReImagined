//! Freeing big values off the tick.
/// Free `value` off the tick: a big copy's bricks (each with its name,
/// events and lights) or a batch of collision shapes take a while to
/// free, as long as they took to build. A thread of their own frees them
/// in the background.
pub fn drop_later<T: Send + 'static>(value: T) {
    use std::sync::{OnceLock, mpsc};
    type Trash = Box<dyn Send>;
    static BIN: OnceLock<Option<mpsc::Sender<Trash>>> = OnceLock::new();
    let bin = BIN.get_or_init(|| {
        let (send, receive) = mpsc::channel::<Trash>();
        std::thread::Builder::new()
            .name("drop-later".into())
            .spawn(move || receive.into_iter().for_each(drop))
            .ok()
            .map(|_| send)
    });
    // No thread to hand it to: free it here.
    if let Some(bin) = bin {
        let _ = bin.send(Box::new(value));
    }
}
