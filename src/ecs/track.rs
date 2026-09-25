use crate::ecs::types::{Keyframe, next_edit_seq};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackError {
    Empty,
    Locked,
    OutOfRange,
    Occupied,
    NoRoom,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Location {
    pub section: usize,
    pub u: f32,
}

pub trait Track {
    fn time_controlled(&self) -> bool;
    fn time_of(&self, index: usize) -> i32;
    fn index_of(&self, frame: i32) -> Option<usize>;
    fn evaluate(&self, frame: i32, fallback: f32) -> f32;
    fn locate(&self, frame: i32) -> Option<Location>;
    fn seed_start(&mut self, start: i32, value: f32);
    fn add_end(&mut self, end: i32) -> Result<usize, TrackError>;
    fn insert_point(&mut self, frame: i32) -> Result<usize, TrackError>;
    fn remove_point(&mut self, index: usize) -> Result<Keyframe, TrackError>;
    fn move_point(&mut self, index: usize, frame: i32) -> Result<i32, TrackError>;
    fn move_key(&mut self, index: usize, frame: i32) -> Result<i32, TrackError>;
    fn move_range(&mut self, first: usize, last: usize, delta: i32) -> Result<i32, TrackError>;
    fn move_control(&mut self, index: usize, time: i32) -> Result<i32, TrackError>;
    fn set_time_control(&mut self, on: bool);
    fn bind_range(&mut self, start: i32, end: i32);
    fn shift(&mut self, delta: i32);
    fn split_at_frame(&mut self, frame: i32, fallback: f32) -> Vec<Keyframe>;
    fn equalize(&mut self) -> Result<(), TrackError>;
}

impl Track for Vec<Keyframe> {
    fn time_controlled(&self) -> bool {
        self.len() >= 2 && self.iter().all(|k| k.control_frame.is_some())
    }

    fn time_of(&self, index: usize) -> i32 {
        let Some(k) = self.get(index) else {
            return 0;
        };
        if self.time_controlled() {
            k.control_frame.unwrap_or(k.frame)
        } else {
            k.frame
        }
    }

    fn index_of(&self, frame: i32) -> Option<usize> {
        self.iter().position(|k| k.frame == frame)
    }

    fn evaluate(&self, frame: i32, fallback: f32) -> f32 {
        let Some(first) = self.first() else {
            return fallback;
        };
        let Some(engine) = crate::easings::loader::by_id(&first.engine_id) else {
            return fallback;
        };
        let controlled = self.time_controlled();
        let raw: Vec<(i32, f32, Vec<u8>)> = self
            .iter()
            .map(|key| {
                let time = if controlled {
                    key.control_frame.unwrap_or(key.frame)
                } else {
                    key.frame
                };
                (time, key.value, key.engine_payload.clone())
            })
            .collect();
        engine.evaluate(&raw, frame, fallback)
    }

    fn locate(&self, frame: i32) -> Option<Location> {
        let n = self.len();
        if n < 2 {
            return None;
        }
        if frame <= self.time_of(0) {
            return Some(Location { section: 0, u: 0.0 });
        }
        if frame >= self.time_of(n - 1) {
            return Some(Location {
                section: n - 2,
                u: 1.0,
            });
        }
        let section = (0..n - 1)
            .rev()
            .find(|&i| self.time_of(i) <= frame)
            .unwrap_or(0);
        let span = (self.time_of(section + 1) - self.time_of(section)).max(1) as f32;
        Some(Location {
            section,
            u: (frame - self.time_of(section)) as f32 / span,
        })
    }

    fn seed_start(&mut self, start: i32, value: f32) {
        if self.is_empty() {
            self.push(Keyframe::new(
                start,
                value,
                "neoutl-easing-standard".to_owned(),
                Vec::new(),
            ));
        }
    }

    fn add_end(&mut self, end: i32) -> Result<usize, TrackError> {
        let Some(last) = self.last() else {
            return Err(TrackError::Empty);
        };
        if self.len() >= 2 {
            return Err(TrackError::Occupied);
        }
        if end <= self.first().map_or(end, |key| key.frame) {
            return Err(TrackError::NoRoom);
        }
        let mut key = last.clone();
        key.frame = end;
        key.edit_seq = next_edit_seq();
        key.control_frame = self[0].control_frame.map(|_| end);
        self.push(key);
        Ok(1)
    }

    fn insert_point(&mut self, frame: i32) -> Result<usize, TrackError> {
        let n = self.len();
        if n == 0 {
            return Err(TrackError::Empty);
        }
        if n < 2
            || frame <= self.first().map_or(frame, |key| key.frame)
            || frame >= self.last().map_or(frame, |key| key.frame)
        {
            return Err(TrackError::OutOfRange);
        }
        if self.index_of(frame).is_some() {
            return Err(TrackError::Occupied);
        }
        let s = self.iter().rposition(|k| k.frame < frame).unwrap_or(0);
        let value = self.evaluate(frame, self[s].value);
        let controlled = self.time_controlled();
        if controlled && !(self.time_of(s) < frame && frame < self.time_of(s + 1)) {
            return Err(TrackError::NoRoom);
        }
        let mut key = self[s].clone();
        key.frame = frame;
        key.value = value;
        key.edit_seq = next_edit_seq();
        key.control_frame = controlled.then_some(frame);
        self.insert(s + 1, key);
        Ok(s + 1)
    }

    fn remove_point(&mut self, index: usize) -> Result<Keyframe, TrackError> {
        if index == 0 {
            return Err(TrackError::Locked);
        }
        if index >= self.len() {
            return Err(TrackError::OutOfRange);
        }
        Ok(Vec::remove(self, index))
    }

    fn move_key(&mut self, index: usize, frame: i32) -> Result<i32, TrackError> {
        if index >= self.len() {
            return Err(TrackError::OutOfRange);
        }
        let lo = if index == 0 {
            i32::MIN
        } else {
            self.get(index - 1)
                .map_or(i32::MIN, |key| key.frame.saturating_add(1))
        };
        let hi = if index + 1 == self.len() {
            i32::MAX
        } else {
            self.get(index + 1)
                .map_or(i32::MAX, |key| key.frame.saturating_sub(1))
        };
        if lo > hi {
            return Err(TrackError::NoRoom);
        }
        let moved = frame.clamp(lo, hi);
        let Some(key) = self.get_mut(index) else {
            return Err(TrackError::OutOfRange);
        };
        key.frame = moved;
        key.edit_seq = next_edit_seq();
        Ok(moved)
    }

    fn move_point(&mut self, index: usize, frame: i32) -> Result<i32, TrackError> {
        if index == 0 || index + 1 >= self.len() {
            return Err(TrackError::Locked);
        }
        self.move_key(index, frame)
    }

    fn move_range(&mut self, first: usize, last: usize, delta: i32) -> Result<i32, TrackError> {
        let n = self.len();
        if first == 0 || last >= n.saturating_sub(1) {
            return Err(TrackError::Locked);
        }
        if first > last {
            return Err(TrackError::OutOfRange);
        }
        let controlled = self.time_controlled();
        let Some(previous) = self.get(first - 1) else {
            return Err(TrackError::OutOfRange);
        };
        let Some(first_key) = self.get(first) else {
            return Err(TrackError::OutOfRange);
        };
        let Some(last_key) = self.get(last) else {
            return Err(TrackError::OutOfRange);
        };
        let Some(next) = self.get(last + 1) else {
            return Err(TrackError::OutOfRange);
        };
        let mut lo = previous
            .frame
            .saturating_add(1)
            .saturating_sub(first_key.frame);
        let mut hi = next.frame.saturating_sub(1).saturating_sub(last_key.frame);
        if controlled {
            lo = lo.max(self.time_of(first - 1) + 1 - self.time_of(first));
            hi = hi.min(self.time_of(last + 1) - 1 - self.time_of(last));
        }
        if lo > hi {
            return Err(TrackError::NoRoom);
        }
        let applied = delta.clamp(lo, hi);
        let Some(keys) = self.get_mut(first..=last) else {
            return Err(TrackError::OutOfRange);
        };
        for k in keys {
            k.frame += applied;
            k.control_frame = k.control_frame.map(|c| c + applied);
            k.edit_seq = next_edit_seq();
        }
        Ok(applied)
    }

    fn move_control(&mut self, index: usize, time: i32) -> Result<i32, TrackError> {
        if !self.time_controlled() || index == 0 || index + 1 >= self.len() {
            return Err(TrackError::Locked);
        }
        let lo = self.time_of(index - 1) + 1;
        let hi = self.time_of(index + 1) - 1;
        if lo > hi {
            return Err(TrackError::NoRoom);
        }
        let moved = time.clamp(lo, hi);
        self[index].control_frame = Some(moved);
        Ok(moved)
    }

    fn set_time_control(&mut self, on: bool) {
        for k in self.iter_mut() {
            k.control_frame = on.then_some(k.frame);
        }
    }

    fn bind_range(&mut self, start: i32, end: i32) {
        let n = self.len();
        if n == 0 {
            return;
        }
        let controlled = self.time_controlled();
        self[0].frame = start;
        if n == 1 {
            self[0].control_frame = None;
            return;
        }
        if end <= start {
            self.truncate(1);
            self[0].control_frame = None;
            return;
        }
        self[n - 1].frame = end;
        let mut index = 0;
        self.retain(|k| {
            index += 1;
            index == 1 || index == n || (start < k.frame && k.frame < end)
        });
        if controlled {
            let last = self.len() - 1;
            self[0].control_frame = Some(start);
            self[last].control_frame = Some(end);
            let ordered = (0..self.len() - 1).all(|i| self.time_of(i) < self.time_of(i + 1));
            if !ordered {
                self.set_time_control(false);
            }
        }
    }

    fn shift(&mut self, delta: i32) {
        for k in self.iter_mut() {
            k.frame += delta;
            k.control_frame = k.control_frame.map(|c| c + delta);
        }
    }

    fn split_at_frame(&mut self, frame: i32, fallback: f32) -> Vec<Keyframe> {
        let n = self.len();
        if n == 0 {
            return Vec::new();
        }
        let value = self.evaluate(frame, fallback);
        let source = self
            .iter()
            .rposition(|k| k.frame <= frame)
            .map_or_else(|| self[0].clone(), |i| self[i].clone());
        let had_end = n >= 2 && self[n - 1].frame >= frame;
        let mut head = source.clone();
        head.frame = frame;
        head.value = value;
        head.edit_seq = next_edit_seq();
        head.control_frame = None;
        let mut second = vec![head];
        second.extend(
            self.iter()
                .filter(|k| k.frame > frame)
                .cloned()
                .map(|mut k| {
                    k.control_frame = None;
                    k
                }),
        );
        self.retain(|k| k.frame < frame);
        if had_end {
            let mut end = source;
            end.frame = frame;
            end.value = value;
            end.edit_seq = next_edit_seq();
            self.push(end);
        }
        self.set_time_control(false);
        second
    }

    fn equalize(&mut self) -> Result<(), TrackError> {
        let n = self.len();
        if n < 3 {
            return Ok(());
        }
        let steps = (n - 1) as i64;
        let controlled = self.time_controlled();
        let (first, last) = (self.time_of(0), self.time_of(n - 1));
        let dur = (last - first) as i64;
        if dur < steps {
            return Err(TrackError::NoRoom);
        }
        let target =
            |i: usize, base: i32| base + ((dur * i as i64 * 2 + steps) / (2 * steps)) as i32;
        let (frame_first, frame_last) = (self[0].frame, self[n - 1].frame);
        let frame_dur = (frame_last - frame_first) as i64;
        if frame_dur < steps {
            return Err(TrackError::NoRoom);
        }
        for i in 1..n - 1 {
            self[i].frame = frame_first + ((frame_dur * i as i64 * 2 + steps) / (2 * steps)) as i32;
            if controlled {
                self[i].control_frame = Some(target(i, first));
            }
            self[i].edit_seq = next_edit_seq();
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(frame: i32, value: f32) -> Keyframe {
        Keyframe::new(
            frame,
            value,
            "neoutl-easing-standard".to_owned(),
            Vec::new(),
        )
    }

    fn frames(track: &[Keyframe]) -> Vec<i32> {
        track.iter().map(|k| k.frame).collect()
    }

    #[test]
    fn seed_and_add_end() {
        let mut t: Vec<Keyframe> = Vec::new();
        t.seed_start(10, 5.0);
        t.seed_start(20, 9.0);
        assert_eq!(frames(&t), [10]);
        assert_eq!(t.add_end(10), Err(TrackError::NoRoom));
        assert_eq!(t.add_end(110), Ok(1));
        assert_eq!(t.add_end(120), Err(TrackError::Occupied));
        assert_eq!(t[1].value, 5.0);
    }

    #[test]
    fn insert_move_remove() {
        let mut t = vec![key(0, 0.0), key(100, 100.0)];
        assert_eq!(t.insert_point(30), Ok(1));
        assert_eq!(t.insert_point(30), Err(TrackError::Occupied));
        assert_eq!(t.insert_point(100), Err(TrackError::OutOfRange));
        assert_eq!(t.insert_point(60), Ok(2));
        assert_eq!(t.move_point(1, 500), Ok(59));
        assert_eq!(t.move_point(0, 5), Err(TrackError::Locked));
        assert_eq!(t.move_point(3, 5), Err(TrackError::Locked));
        assert_eq!(t.remove_point(0), Err(TrackError::Locked));
        assert!(t.remove_point(1).is_ok());
        assert_eq!(frames(&t), [0, 60, 100]);
    }

    #[test]
    fn range_bind_equalize_split() {
        let mut t = vec![key(0, 0.0), key(30, 1.0), key(60, 2.0), key(100, 3.0)];
        assert_eq!(t.move_range(1, 2, 1000), Ok(39));
        assert_eq!(frames(&t), [0, 69, 99, 100]);
        assert_eq!(t.move_range(1, 2, -1000), Ok(-68));
        t.bind_range(20, 90);
        assert_eq!(frames(&t), [20, 31, 90]);
        let mut t = vec![key(0, 0.0), key(10, 0.0), key(90, 0.0), key(100, 0.0)];
        assert_eq!(t.equalize(), Ok(()));
        assert_eq!(frames(&t), [0, 33, 67, 100]);
        let tail = t.split_at_frame(50, 0.0);
        assert_eq!(frames(&t), [0, 33, 50]);
        assert_eq!(frames(&tail), [50, 67, 100]);
    }

    #[test]
    fn time_control() {
        let mut t = vec![key(0, 0.0), key(30, 1.0), key(100, 0.0)];
        t.set_time_control(true);
        assert!(t.time_controlled());
        assert_eq!(t.move_control(1, 70), Ok(70));
        assert_eq!(t.locate(35).map(|l| l.section), Some(0));
        assert_eq!(t.locate(80).map(|l| l.section), Some(1));
        t.bind_range(0, 50);
        assert!(!t.time_controlled());
    }
}
