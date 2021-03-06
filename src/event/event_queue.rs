use super::Timed;
use crate::event::EventHandler;
#[cfg(test)]
use crate::test_utilities::{DummyEventHandler, TestPlugin};
use crate::ContextualAudioRenderer;
use std::cmp::Ordering;
use std::collections::VecDeque;
use std::ops::{Deref, Index, IndexMut};
use vecstorage::{VecGuard, VecStorage};

pub struct EventQueue<T> {
    queue: VecDeque<Timed<T>>,
}

pub enum EventCollisionHandling {
    InsertNewBeforeOld,
    InsertNewAfterOld,
    IgnoreNew,
    RemoveOld,
}

pub trait HandleEventCollision<T> {
    fn decide_on_collision(&self, old_event: &T, new_event: &T) -> EventCollisionHandling;
}

pub struct AlwaysInsertNewBeforeOld;
impl<T> HandleEventCollision<T> for AlwaysInsertNewBeforeOld {
    #[inline(always)]
    fn decide_on_collision(&self, _old_event: &T, _new_event: &T) -> EventCollisionHandling {
        EventCollisionHandling::InsertNewBeforeOld
    }
}

pub struct AlwaysInsertNewAfterOld;
impl<T> HandleEventCollision<T> for AlwaysInsertNewAfterOld {
    #[inline(always)]
    fn decide_on_collision(&self, _old_event: &T, _new_event: &T) -> EventCollisionHandling {
        EventCollisionHandling::InsertNewAfterOld
    }
}

pub struct AlwaysIgnoreNew;
impl<T> HandleEventCollision<T> for AlwaysIgnoreNew {
    #[inline(always)]
    fn decide_on_collision(&self, _old_event: &T, _new_event: &T) -> EventCollisionHandling {
        EventCollisionHandling::IgnoreNew
    }
}

pub struct AlwaysRemoveOld;
impl<T> HandleEventCollision<T> for AlwaysRemoveOld {
    #[inline(always)]
    fn decide_on_collision(&self, _old_event: &T, _new_event: &T) -> EventCollisionHandling {
        EventCollisionHandling::RemoveOld
    }
}

impl<T> Index<usize> for EventQueue<T> {
    type Output = Timed<T>;

    fn index(&self, index: usize) -> &Self::Output {
        &self.queue[index]
    }
}

impl<T> IndexMut<usize> for EventQueue<T> {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.queue[index]
    }
}

impl<T> EventQueue<T> {
    #[cfg(test)]
    pub fn from_vec(events: Vec<Timed<T>>) -> Self {
        Self {
            queue: events.into(),
        }
    }

    /// # Panics
    /// Panics if `capacity == 0`.
    pub fn new(capacity: usize) -> Self {
        assert!(capacity > 0);
        Self {
            queue: VecDeque::with_capacity(capacity),
        }
    }

    /// Queue a new event.
    /// When the buffer is full, an element may be removed from the queue to make some room.
    /// This element is returned.
    pub fn queue_event<H>(&mut self, new_event: Timed<T>, collision_decider: H) -> Option<Timed<T>>
    where
        H: HandleEventCollision<T>,
    {
        let mut new_event = new_event;
        let result;
        if self.queue.len() >= self.queue.capacity() {
            // Note: self.queue.capacity() > 0, so self.queue is not empty.
            // TODO: Log an error.
            // We remove the first event to come, in this way,
            // we are sure we are not skipping the "last" event,
            // because we assume that the state of the first event
            // is only temporarily, and the state of the last event
            // may remain forever. For this reason, it is safer to
            // remove the first event
            if new_event.time_in_frames > self.queue[0].time_in_frames {
                result = self.queue.pop_front();
            } else {
                return Some(new_event);
            }
        } else {
            result = None;
        }
        // If we are at this point, we can assume that we can insert at least one more event.
        debug_assert!(self.queue.len() < self.queue.capacity());

        let mut insert_index = 0;
        for read_event in self.queue.iter_mut() {
            match read_event.time_in_frames.cmp(&new_event.time_in_frames) {
                Ordering::Less => {
                    insert_index += 1;
                }
                Ordering::Equal => {
                    match collision_decider.decide_on_collision(&read_event.event, &new_event.event)
                    {
                        EventCollisionHandling::IgnoreNew => {
                            return Some(new_event);
                        }
                        EventCollisionHandling::InsertNewBeforeOld => {
                            break;
                        }
                        EventCollisionHandling::InsertNewAfterOld => {
                            insert_index += 1;
                        }
                        EventCollisionHandling::RemoveOld => {
                            std::mem::swap(&mut read_event.event, &mut new_event.event);
                            return Some(new_event);
                        }
                    }
                }
                Ordering::Greater => {
                    break;
                }
            }
        }
        self.queue.insert(insert_index, new_event);

        result
    }

    /// Remove all events before, but not on, this threshold.
    ///
    /// # Note about usage in real-time context
    /// If `T` implements drop, the elements that are removed are dropped.
    /// This may cause memory de-allocation, which you want to avoid in
    /// the real-time part of your library.
    pub fn forget_before(&mut self, threshold: u32)
    where
        T: Copy,
    {
        self.queue.retain(|x| x.time_in_frames >= threshold);
    }

    /// Remove all events from the queue.
    ///
    /// # Note about usage in real-time context
    /// If `T` implements drop, the elements that are removed are dropped.
    /// This may cause memory de-allocation, which you want to avoid in
    /// the real-time part of your library.
    pub fn clear(&mut self) {
        self.queue.clear()
    }

    /// Shift time forward by `new_zero_time` frames.
    ///
    /// # Panics
    /// Panics in debug mode when at least one event has a `time_in_frames`
    /// that is < `new_zero_time`.  
    pub fn shift_time(&mut self, new_zero_time: u32) {
        for event in self.queue.iter_mut() {
            event.time_in_frames -= new_zero_time;
        }
    }

    pub fn get_last_before(&self, time: u32) -> Option<&Timed<T>> {
        if let Some(index) = self.queue.iter().rposition(|e| e.time_in_frames < time) {
            self.queue.get(index)
        } else {
            None
        }
    }

    pub fn first(&self) -> Option<&Timed<T>> {
        self.queue.get(0)
    }

    fn render<'storage, 's, 'chunk, S, R, C>(
        start: usize,
        stop: usize,
        input_storage: &'storage mut VecStorage<&'static [S]>,
        output_storage: &'storage mut VecStorage<&'static mut [S]>,
        inputs: &[&[S]],
        outputs: &mut [&mut [S]],
        renderer: &mut R,
        context: &mut C,
    ) where
        S: 'static,
        R: ContextualAudioRenderer<S, C>,
    {
        let input_guard = mid(input_storage, inputs, start, stop);
        let mut output_guard = mid_mut(output_storage, outputs, start, stop);
        renderer.render_buffer(&input_guard, &mut output_guard, context);
    }

    pub fn split<'storage, 's, 'chunk, S, R, C>(
        &mut self,
        input_storage: &'storage mut VecStorage<&'static [S]>,
        output_storage: &'storage mut VecStorage<&'static mut [S]>,
        inputs: &[&[S]],
        outputs: &'s mut [&'s mut [S]],
        renderer: &mut R,
        context: &mut C,
    ) where
        S: 'static,
        R: ContextualAudioRenderer<S, C> + EventHandler<T>,
        T: std::fmt::Debug,
    {
        let buffer_length = if inputs.len() > 0 {
            inputs[0].len()
        } else if outputs.len() > 0 {
            outputs[0].len()
        } else {
            todo!();
        };
        let mut last_event_time = 0;
        loop {
            if let Some(ref first) = self.queue.get(0) {
                if first.time_in_frames as usize >= buffer_length {
                    break;
                }
            } else {
                break;
            };
            let Timed {
                time_in_frames: event_time,
                event,
            } = self.queue.pop_front().expect("event queue is not empty");
            if event_time == last_event_time {
                renderer.handle_event(event);
                continue;
            }
            Self::render(
                last_event_time as usize,
                event_time as usize,
                input_storage,
                output_storage,
                inputs,
                outputs,
                renderer,
                context,
            );
            renderer.handle_event(event);
            last_event_time = event_time;
        }
        if (last_event_time as usize) < buffer_length {
            Self::render(
                last_event_time as usize,
                buffer_length,
                input_storage,
                output_storage,
                inputs,
                outputs,
                renderer,
                context,
            );
        };
    }
}

#[test]
fn split_works() {
    let mut test_plugin = TestPlugin::new(
        vec![
            audio_chunk![[11, 12], [21, 22]],
            audio_chunk![[13, 14], [23, 24]],
        ],
        vec![
            audio_chunk![[110, 120], [210, 220]],
            audio_chunk![[130, 140], [230, 240]],
        ],
        vec![vec![1, 2], vec![3, 4]],
        vec![vec![], vec![]],
        (),
    );
    let input = audio_chunk![[11, 12, 13, 14], [21, 22, 23, 24]];
    let mut output = audio_chunk![[0, 0, 0, 0], [0, 0, 0, 0]];
    let events = vec![
        Timed {
            time_in_frames: 0,
            event: 1,
        },
        Timed {
            time_in_frames: 0,
            event: 2,
        },
        Timed {
            time_in_frames: 2,
            event: 3,
        },
        Timed {
            time_in_frames: 2,
            event: 4,
        },
        Timed {
            time_in_frames: 4,
            event: 5,
        },
    ];
    let mut queue = EventQueue::from_vec(events);
    let mut input_storage = VecStorage::with_capacity(2);
    let mut output_storage = VecStorage::with_capacity(2);
    let mut result_event_handler = DummyEventHandler;
    queue.split(
        &mut input_storage,
        &mut output_storage,
        &input.as_slices(),
        &mut output.as_mut_slices(),
        &mut test_plugin,
        &mut result_event_handler,
    )
}

#[test]
fn split_works_with_empty_event_queue() {
    let mut test_plugin = TestPlugin::<_, (), _>::new(
        vec![audio_chunk![[11, 12, 13, 14], [21, 22, 23, 24]]],
        vec![audio_chunk![[110, 120, 130, 140], [210, 220, 230, 240]]],
        vec![vec![]],
        vec![vec![]],
        (),
    );
    let input = audio_chunk![[11, 12, 13, 14], [21, 22, 23, 24]];
    let mut output = audio_chunk![[0, 0, 0, 0], [0, 0, 0, 0]];
    let events: Vec<()> = vec![];
    let mut queue = EventQueue::new(1);
    let mut input_storage = VecStorage::with_capacity(2);
    let mut output_storage = VecStorage::with_capacity(2);
    let mut result_event_handler = DummyEventHandler;
    queue.split(
        &mut input_storage,
        &mut output_storage,
        &input.as_slices(),
        &mut output.as_mut_slices(),
        &mut test_plugin,
        &mut result_event_handler,
    )
}

impl<T> Deref for EventQueue<T> {
    type Target = VecDeque<Timed<T>>;

    fn deref(&self) -> &Self::Target {
        &self.queue
    }
}

// TODO: Move to a better place in the module hierarchy.
pub fn mid<'storage, 'chunk, 's, S>(
    storage: &'storage mut VecStorage<&'static [S]>,
    chunk: &'chunk [&'s [S]],
    start: usize,
    end: usize,
) -> VecGuard<'storage, &'static [S], &'chunk [S]> {
    let mut remaining_chunk = chunk;
    let mut guard = storage.vec_guard();
    let mut len = remaining_chunk.len();
    while len > 0 {
        let (first_channel, other_channels) = remaining_chunk.split_at(1);
        let channel = &(first_channel[0]);
        let (first, _) = channel.split_at(end);
        let (_, middle) = first.split_at(start);
        guard.push(middle);
        remaining_chunk = other_channels;
        len = remaining_chunk.len();
    }
    guard
}

// TODO: Move to a better place in the module hierarchy.
///
/// ## Panics
/// Panics if `start` > `end` or if `end` > the length of any item in `chunk`.
pub fn mid_mut<'storage, 'chunk, 's, S>(
    storage: &'storage mut VecStorage<&'static mut [S]>,
    chunk: &'chunk mut [&'s mut [S]],
    start: usize,
    end: usize,
) -> VecGuard<'storage, &'static mut [S], &'chunk mut [S]> {
    let mut remaining_chunk = chunk;
    let mut guard = storage.vec_guard();
    let mut len = remaining_chunk.len();
    while len > 0 {
        let (first_channel, other_channels) = remaining_chunk.split_at_mut(1);
        let channel = &mut (first_channel[0]);
        let (first, _) = channel.split_at_mut(end);
        let (_, middle) = first.split_at_mut(start);
        guard.push(middle);
        remaining_chunk = other_channels;
        len = remaining_chunk.len();
    }
    guard
}

#[test]
fn mid_mut_works() {
    let mut storage = VecStorage::with_capacity(2);
    let mut channel1 = [11, 12, 13, 14];
    let mut channel2 = [21, 22, 23, 24];
    let chunk: &mut [&mut [_]] = &mut [&mut channel1, &mut channel2];
    {
        let guard = mid_mut(&mut storage, chunk, 0, 0);
        assert_eq!(guard.len(), 2);
        assert!(guard[0].is_empty());
        assert!(guard[1].is_empty());
    }
    {
        let guard = mid_mut(&mut storage, chunk, 0, 1);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &mut [11]);
        assert_eq!(guard[1], &mut [21]);
    }
    {
        let guard = mid_mut(&mut storage, chunk, 0, 2);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &mut [11, 12]);
        assert_eq!(guard[1], &mut [21, 22]);
    }
    {
        let guard = mid_mut(&mut storage, chunk, 1, 2);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &mut [12]);
        assert_eq!(guard[1], &mut [22]);
    }
}

#[test]
fn mid_works() {
    let mut storage = VecStorage::with_capacity(2);
    let channel1 = [11, 12, 13, 14];
    let channel2 = [21, 22, 23, 24];
    let chunk: &[&[_]] = &[&channel1, &channel2];
    {
        let guard = mid(&mut storage, chunk, 0, 0);
        assert_eq!(guard.len(), 2);
        assert!(guard[0].is_empty());
        assert!(guard[1].is_empty());
    }
    {
        let guard = mid(&mut storage, chunk, 0, 1);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &[11]);
        assert_eq!(guard[1], &[21]);
    }
    {
        let guard = mid(&mut storage, chunk, 0, 2);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &[11, 12]);
        assert_eq!(guard[1], &[21, 22]);
    }
    {
        let guard = mid(&mut storage, chunk, 1, 2);
        assert_eq!(guard.len(), 2);
        assert_eq!(guard[0], &[12]);
        assert_eq!(guard[1], &[22]);
    }
}

#[test]
fn eventqueue_queue_event_new_event_ignored_when_already_full_and_new_event_comes_first() {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    // Check our assumption:
    assert_eq!(queue.queue.capacity(), queue.queue.len());

    // Act
    queue.queue_event(Timed::new(3, 9), AlwaysIgnoreNew);

    // Assert:
    assert_eq!(queue.queue, initial_buffer);
}

#[test]
fn event_queue_queue_event_first_event_removed_when_already_full_and_new_event_after_first() {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    // Check our assumption:
    assert_eq!(queue.queue.capacity(), queue.queue.len());

    // Act
    queue.queue_event(Timed::new(5, 25), AlwaysInsertNewAfterOld);

    // Assert:
    assert_eq!(
        queue.queue,
        vec![Timed::new(5, 25), Timed::new(6, 36), Timed::new(7, 49),]
    );
}

#[test]
fn eventqueue_queue_event_new_event_inserted_at_correct_location() {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    queue.queue_event(Timed::new(5, 25), AlwaysInsertNewAfterOld);

    // Assert:
    assert_eq!(
        queue.queue,
        vec![
            Timed::new(4, 16),
            Timed::new(5, 25),
            Timed::new(6, 36),
            Timed::new(7, 49),
        ]
    );
}

#[test]
fn eventqueue_queue_event_with_always_ignore_new_new_event_ignored_when_already_event_at_that_location(
) {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    queue.queue_event(Timed::new(6, 25), AlwaysIgnoreNew);

    // Assert:
    assert_eq!(queue.queue, initial_buffer);
}

#[test]
fn eventqueue_queue_event_with_always_ignore_old_old_event_ignored_when_already_event_at_that_location(
) {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let expected_buffer = vec![Timed::new(4, 16), Timed::new(6, 25), Timed::new(7, 49)];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    let result = queue.queue_event(Timed::new(6, 25), AlwaysRemoveOld);

    assert_eq!(result, Some(Timed::new(6, 36)));

    // Assert:
    assert_eq!(queue.queue, expected_buffer);
}

#[test]
fn eventqueue_queue_event_with_always_insert_new_after_old() {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let expected_buffer = vec![
        Timed::new(4, 16),
        Timed::new(6, 36),
        Timed::new(6, 25),
        Timed::new(7, 49),
    ];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    let result = queue.queue_event(Timed::new(6, 25), AlwaysInsertNewAfterOld);

    assert_eq!(result, None);

    // Assert:
    assert_eq!(queue.queue, expected_buffer);
}

#[test]
fn eventqueue_queue_event_with_always_insert_new_after_old_with_doubles() {
    let initial_buffer = vec![Timed::new(6, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let expected_buffer = vec![
        Timed::new(6, 16),
        Timed::new(6, 36),
        Timed::new(6, 25),
        Timed::new(7, 49),
    ];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    let result = queue.queue_event(Timed::new(6, 25), AlwaysInsertNewAfterOld);

    assert_eq!(result, None);

    // Assert:
    assert_eq!(queue.queue, expected_buffer);
}

#[test]
fn eventqueue_queue_event_with_always_insert_new_before_old() {
    let initial_buffer = vec![Timed::new(4, 16), Timed::new(6, 36), Timed::new(7, 49)];
    let expected_buffer = vec![
        Timed::new(4, 16),
        Timed::new(6, 25),
        Timed::new(6, 36),
        Timed::new(7, 49),
    ];
    let mut queue = EventQueue::from_vec(initial_buffer.clone());
    queue.queue.reserve(1);

    // Act
    let result = queue.queue_event(Timed::new(6, 25), AlwaysInsertNewBeforeOld);

    assert_eq!(result, None);

    // Assert:
    assert_eq!(queue.queue, expected_buffer);
}

#[test]
fn eventqueue_forget_before() {
    let mut queue = EventQueue::from_vec({
        vec![
            Timed::new(4, 16),
            Timed::new(6, 36),
            Timed::new(7, 49),
            Timed::new(8, 64),
        ]
    });
    queue.forget_before(7);
    assert_eq!(queue.queue, vec![Timed::new(7, 49), Timed::new(8, 64),]);
}

#[test]
fn eventqueue_forget_everything() {
    let mut queue = EventQueue::from_vec({
        vec![
            Timed::new(4, 16),
            Timed::new(6, 36),
            Timed::new(7, 49),
            Timed::new(8, 64),
        ]
    });
    queue.forget_before(9);
    assert_eq!(queue.queue, Vec::new());
}
