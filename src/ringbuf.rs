use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Arc;

pub struct RingBuf<T> {
    buffer: Vec<T>,
    capacity: usize,
    one_was_dropped: AtomicBool,
}

impl<T: Default + Clone> RingBuf<T> {
    pub fn new(capacity: usize, default: T) -> Self {
        assert!(capacity != 1, "Use a RwLock for capacity 1");

        RingBuf {
            buffer: vec![default; capacity],
            capacity,
            one_was_dropped: AtomicBool::new(false),
        }
    }

    pub fn create_channel_with_default_value(
        capacity: usize,
        default: T,
    ) -> (RingBufProducer<T>, RingBufConsumer<T>) {
        let ringbuf = Arc::new(RingBuf::new(capacity, default));

        let (next_write_pos_sink, next_write_pos_receiver) = std::sync::mpsc::channel();
        let (last_read_pos_sender, last_read_pos_receiver) = std::sync::mpsc::channel();

        let producer =
            RingBufProducer::new(ringbuf.clone(), next_write_pos_sink, last_read_pos_receiver);
        let consumer = RingBufConsumer::new(
            ringbuf.clone(),
            next_write_pos_receiver,
            last_read_pos_sender,
        );

        (producer, consumer)
    }

    pub fn create_channel(capacity: usize) -> (RingBufProducer<T>, RingBufConsumer<T>) {
        Self::create_channel_with_default_value(capacity, Default::default())
    }
}

pub struct RingBufProducer<T> {
    ringbuf: Arc<RingBuf<T>>,
    next_write_pos_sink: Sender<usize>,
    last_read_pos: Receiver<usize>,
    next_write_pos: usize,
    lastknown_last_read_pos: usize,
}

impl<T> RingBufProducer<T> {
    fn new(
        ringbuf: Arc<RingBuf<T>>,
        next_write_pos_sink: Sender<usize>,
        last_read_pos: Receiver<usize>,
    ) -> Self {
        Self {
            ringbuf,
            next_write_pos_sink,
            last_read_pos,
            next_write_pos: 0,
            lastknown_last_read_pos: 0,
        }
    }

    pub fn cancel(&mut self) {
        self.ringbuf.one_was_dropped.store(true, Ordering::Relaxed);
        self.next_write_pos_sink.send(self.next_write_pos).unwrap();
    }

    pub fn with_next_buffer<F: FnMut(&mut T) -> R, R>(
        &mut self,
        mut func: F,
    ) -> std::result::Result<R, ()> {
        for last_read_pos in
            std::iter::once(self.lastknown_last_read_pos).chain(self.last_read_pos.iter())
        {
            if self.ringbuf.one_was_dropped.load(Ordering::Relaxed) {
                return Err(());
            }

            if (self.next_write_pos - last_read_pos) < self.ringbuf.capacity {
                self.lastknown_last_read_pos = last_read_pos;
                break;
            }
        }

        let pos = self.next_write_pos % self.ringbuf.capacity;

        let ret = unsafe { func(&mut Arc::get_mut_unchecked(&mut self.ringbuf).buffer[pos]) };

        self.next_write_pos += 1;
        self.next_write_pos_sink.send(self.next_write_pos).unwrap();

        Ok(ret)
    }
}

impl<T> Drop for RingBufProducer<T> {
    fn drop(&mut self) {
        self.cancel()
    }
}

pub struct RingBufConsumer<T> {
    ringbuf: Arc<RingBuf<T>>,
    next_write_pos: Receiver<usize>,
    last_read_pos: Sender<usize>,
    next_read_pos: usize,
    lastknown_next_write_pos: usize,
}

impl<T> RingBufConsumer<T> {
    fn new(
        ringbuf: Arc<RingBuf<T>>,
        next_write_pos: Receiver<usize>,
        last_read_pos: Sender<usize>,
    ) -> Self {
        Self {
            ringbuf,
            next_write_pos,
            last_read_pos,
            next_read_pos: 0,
            lastknown_next_write_pos: 0,
        }
    }

    pub fn cancel(&mut self) {
        self.ringbuf.one_was_dropped.store(true, Ordering::Relaxed);
        self.last_read_pos.send(self.next_read_pos - 1).unwrap();
    }

    pub fn with_next_buffer<F: FnMut(&T) -> R, R>(
        &mut self,
        mut func: F,
    ) -> std::result::Result<R, ()> {
        for next_write_pos in
            std::iter::once(self.lastknown_next_write_pos).chain(self.next_write_pos.iter())
        {
            if self.ringbuf.one_was_dropped.load(Ordering::Relaxed) {
                return Err(());
            }

            if next_write_pos > self.next_read_pos {
                self.lastknown_next_write_pos = next_write_pos;
                break;
            }
        }

        let pos = self.next_read_pos % self.ringbuf.capacity;
        let ret = func(&self.ringbuf.buffer[pos]);

        self.last_read_pos.send(self.next_read_pos).unwrap();

        self.next_read_pos += 1;

        Ok(ret)
    }
}

impl<T> Drop for RingBufConsumer<T> {
    fn drop(&mut self) {
        self.cancel()
    }
}
