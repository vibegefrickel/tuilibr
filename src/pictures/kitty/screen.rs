//! The frame engine: the pictures that the terminal stores and their places on the screen. The
//! app puts the places of one frame into [`Placements`]. After the draw of ratatui, one
//! `Screen::flush` or one [`Screen::write_frame`] writes the deletes, the
//! transfers up to the byte cap of a pass in the order of the frame, and the changed placements,
//! in one synchronized update.
//!
//! Each picture goes to the terminal one time, under its id, and each place gets a direct
//! placement on its cells (`c=` and `r=`) with its own placement id (`p=`). The engine uses no
//! Unicode placeholders and no pixel offsets.
//!
//! The terminal keeps each sent picture until a delete. Above the budget of the store, the
//! engine deletes the oldest pictures that the frame does not show. After a clear, a resize or a
//! focus gain, the next flushes send the pictures of the frame again.
//!
//! The self check ([`Screen::with_self_check`]) places with `q=1`. The terminal then answers an
//! error for a placement of a picture that it does not hold. The app reads the answers
//! ([`super::answers`]) and hands each one to [`Screen::answer`]. The next flush sends the lost
//! picture again.

use super::answers::Answer;
use super::protocol;
use super::terminal_images::{TERMINAL_IMAGE_BUDGET, TerminalImages};
use super::{DisplayOptions, EMIT_PASS_BYTES, KittyBackend, Pixels};
use ratatui::layout::Rect;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::io::{self, Write};
use std::time::{Duration, Instant};

/// The most losses of one picture within `LOSS_WINDOW` that the screen answers with a new
/// transfer. When the store of the terminal is smaller than the budget, each transfer can evict
/// another picture on the screen, and its `ENOENT` would bring the next transfer at the speed of
/// the round trip.
const MAX_LOSSES: usize = 3;

/// The time within which `MAX_LOSSES` losses of one picture stop its new transfers.
const LOSS_WINDOW: Duration = Duration::from_secs(10);

/// The start and the end of a synchronized update: the terminal shows the bytes between them in
/// one step. The mode is not a counter, so an app can put one update around the cells of a frame
/// and the flush: the end of the flush ends that update, and a second end does nothing.
pub const BEGIN_SYNC: &[u8] = b"\x1b[?2026h";
pub const END_SYNC: &[u8] = b"\x1b[?2026l";

/// Which placements a flush writes.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Placing {
    /// Only a new or a moved placement goes out. A terminal keeps a placement until a delete.
    #[default]
    Changed,
    /// Every place of the frame goes out on each flush, also an unchanged one. A terminal or a
    /// multiplexer that dropped a placement gets it back with the next flush.
    EveryFrame,
}

/// The layer of a place.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    /// Above the text of the cells, as the terminal draws a picture by default.
    #[default]
    AboveText,
    /// Under the text and the cell backgrounds (`z=` [`super::BELOW_BACKGROUNDS`]), so text,
    /// lines and a colored ground on top stay readable.
    UnderCells,
}

/// One place of a picture in a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Place {
    id: u32,
    cells: Rect,
    layer: Layer,
    /// An older form of the picture (a preview). It keeps the place while `id` waits for its
    /// pass, and it goes when `id` shows.
    earlier: Option<u32>,
}

/// The places of the pictures in the frame that the app draws now, in the order of the screen.
/// The order is the order of the transfers when the byte cap of a pass splits them.
#[derive(Debug, Default)]
pub struct Placements {
    places: Vec<Place>,
    /// The pictures that stay in the terminal without a place in this frame: the trim skips
    /// them. A keep sends nothing.
    kept: Vec<u32>,
    /// An overlay covers the screen, so no picture shows in this frame.
    hidden: bool,
}

impl Placements {
    /// Puts the picture `id` on `cells` in this frame. A second call for the same picture adds a
    /// second place.
    pub fn place(&mut self, id: u32, cells: Rect) {
        self.push(id, cells, Layer::AboveText, None);
    }

    /// Puts the picture `id` on `cells`, in place of its older form `earlier`. While `id` waits
    /// for its pass, `earlier` keeps its placement, or takes `cells` without one, when the
    /// terminal stores it. When `id` shows, the terminal deletes `earlier`.
    pub fn place_over(&mut self, id: u32, earlier: u32, cells: Rect) {
        self.push(id, cells, Layer::AboveText, Some(earlier));
    }

    /// Keeps the stored picture `id` in the terminal without a place, for a later frame. Its age
    /// for the budget stays as it is.
    pub fn keep(&mut self, id: u32) {
        self.kept.push(id);
    }

    /// The places of this frame, in the order of `place`.
    #[cfg(test)]
    pub fn cells(&self) -> Vec<(u32, Rect)> {
        self.places
            .iter()
            .map(|place| (place.id, place.cells))
            .collect()
    }

    fn push(&mut self, id: u32, cells: Rect, layer: Layer, earlier: Option<u32>) {
        if !self.hidden && !cells.is_empty() {
            self.places.push(Place {
                id,
                cells,
                layer,
                earlier,
            });
        }
    }
}

/// The places for apps with other needs: the ground under the text and
/// the overlay.
impl Placements {
    /// Puts the picture `id` on `cells` under the text of the cells, as a ground for text.
    pub fn place_under(&mut self, id: u32, cells: Rect) {
        self.push(id, cells, Layer::UnderCells, None);
    }

    /// An overlay covers the screen: this frame shows no picture. The terminal draws a picture
    /// above the text, so a `Clear` of the cells does not remove it.
    pub fn hide(&mut self) {
        self.places.clear();
        self.hidden = true;
    }
}

/// One synchronized update. The start goes out before the first byte, so an update without
/// bytes writes nothing. The cursor goes back to where ratatui left it. The update counts the
/// pixel bytes of its transfers for the byte cap of a pass. A started update always ends: the
/// drop ends it when an error returned before [`Update::finish`].
pub struct Update<'w, W: Write + ?Sized> {
    out: &'w mut W,
    started: bool,
    pixel_bytes: usize,
    /// The byte cap of this update in place of the cap of the screen.
    pass_bytes: Option<usize>,
}

impl<'w, W: Write + ?Sized> Update<'w, W> {
    pub fn new(out: &'w mut W) -> Self {
        Self {
            out,
            started: false,
            pixel_bytes: 0,
            pass_bytes: None,
        }
    }

    /// An update whose flushes send every picture of the frame, without a byte cap. A redraw
    /// shows the whole screen in one step.
    pub fn uncapped(out: &'w mut W) -> Self {
        let mut update = Self::new(out);
        update.pass_bytes = Some(usize::MAX);
        update
    }

    /// The pixel bytes that this update sent, for the byte cap of the pass.
    pub const fn pixel_bytes(&self) -> usize {
        self.pixel_bytes
    }

    /// Counts the pixel bytes of a transfer that the app wrote into this update itself.
    pub const fn add_pixel_bytes(&mut self, bytes: usize) {
        self.pixel_bytes = self.pixel_bytes.saturating_add(bytes);
    }

    /// Ends the update when it wrote a byte.
    pub fn finish(mut self) -> io::Result<()> {
        self.end()
    }

    /// Writes the end of a started update one time.
    fn end(&mut self) -> io::Result<()> {
        if !self.started {
            return Ok(());
        }
        self.started = false;
        self.out.write_all(b"\x1b8")?;
        self.out.write_all(END_SYNC)?;
        self.out.flush()
    }
}

impl<W: Write + ?Sized> Drop for Update<'_, W> {
    fn drop(&mut self) {
        #[expect(
            clippy::let_underscore_must_use,
            reason = "the error that dropped the update goes to the caller, a second one adds nothing"
        )]
        let _ = self.end();
    }
}

impl<W: Write + ?Sized> Write for Update<'_, W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if !self.started && !buf.is_empty() {
            self.out.write_all(BEGIN_SYNC)?;
            self.out.write_all(b"\x1b7")?;
            self.started = true;
        }
        self.out.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.out.flush()
    }
}

/// The cells of a placement in the terminal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Shown {
    cells: Rect,
    layer: Layer,
}

/// One place of a flush: the picture that shows there, its placement id and its write.
#[derive(Clone, Copy)]
struct Step<'a> {
    id: u32,
    shown: Shown,
    placement: u32,
    /// The pixels that go out with the place (`a=T`).
    send: Option<Pixels<'a>>,
    /// The placement goes out: a new or a moved place, the policy, or new pixels.
    write: bool,
}

/// What the terminal holds: the sent pictures and the cells of their placements. A new `a=p`
/// with the same picture id and placement id moves a placement.
#[derive(Debug)]
pub struct Screen {
    /// The stored pictures with their pixel bytes, the oldest first, and the budget.
    store: TerminalImages,
    /// The pictures that the terminal can still store after a clear: a terminal that kept them
    /// through the clear still needs their delete. They count for no budget.
    maybe_stored: BTreeSet<u32>,
    /// The stored pictures whose pixels go out again at their next placement: after a focus
    /// gain, the terminal or a multiplexer can have dropped them. They still count for the
    /// budget.
    resend: BTreeSet<u32>,
    /// The cells by picture id and placement id.
    placed: BTreeMap<(u32, u32), Shown>,
    /// The size after the last resize. A resize to the same size clears nothing.
    size: Option<(u16, u16)>,
    placing: Placing,
    /// The pixel bytes of new pictures per flush. The first picture of a flush always goes out.
    pass_bytes: usize,
    /// The pictures of the last flush that wait for a later pass, in the order of the frame.
    waiting: Vec<u32>,
    /// The placements ask for the error answers of the terminal (`q=1`).
    self_check: bool,
    /// The terminal lost a picture since the last flush.
    lost_any: bool,
    /// The times of the recent losses by picture id, within `LOSS_WINDOW`.
    losses: BTreeMap<u32, Vec<Instant>>,
    /// The pictures whose `ENOENT` the loss limit ignored, with the end of their window. After
    /// that end, [`Screen::release_held_losses`] marks them lost, and the next flush sends them.
    held: BTreeMap<u32, Instant>,
}

impl Default for Screen {
    fn default() -> Self {
        Self::with_budget(TERMINAL_IMAGE_BUDGET)
    }
}

impl Screen {
    /// A screen that keeps at most `budget` pixel bytes in the terminal. A test sets a small
    /// budget.
    pub fn with_budget(budget: usize) -> Self {
        let mut store = TerminalImages::new();
        store.budget = budget;
        Self {
            store,
            maybe_stored: BTreeSet::new(),
            resend: BTreeSet::new(),
            placed: BTreeMap::new(),
            size: None,
            placing: Placing::default(),
            pass_bytes: EMIT_PASS_BYTES,
            waiting: Vec::new(),
            self_check: false,
            lost_any: false,
            losses: BTreeMap::new(),
            held: BTreeMap::new(),
        }
    }

    /// The same screen with the self check on or off. With the self check, each placement asks
    /// for the error answers of the terminal (`q=1`), and the app hands them to
    /// [`Screen::answer`]. A transfer stays silent (`q=2`), else each one gets an `OK`.
    #[must_use]
    pub const fn with_self_check(mut self, on: bool) -> Self {
        self.self_check = on;
        self
    }

    /// Reads an answer of the terminal. An `ENOENT` for a stored picture marks it lost, and the
    /// next flush that places it sends its pixels again. An answer for a picture that the screen
    /// no longer stores changes nothing. Returns `true` when the screen lost a picture.
    pub fn answer(&mut self, answer: &Answer) -> bool {
        self.answer_at(answer, Instant::now())
    }

    /// `Screen::answer` with the clock of the loss limit. After `MAX_LOSSES` losses of a picture
    /// within `LOSS_WINDOW`, its next `ENOENT` changes nothing, so a test needs no sleep.
    pub fn answer_at(&mut self, answer: &Answer, now: Instant) -> bool {
        if !answer.is_missing() || !self.store.contains(answer.id) {
            return false;
        }
        self.losses.retain(|_, times| {
            times.retain(|&at| now.saturating_duration_since(at) < LOSS_WINDOW);
            !times.is_empty()
        });
        let times = self.losses.entry(answer.id).or_default();
        if let Some(&oldest) = times.first()
            && times.len() >= MAX_LOSSES
        {
            self.held.insert(answer.id, oldest + LOSS_WINDOW);
            return false;
        }
        times.push(now);
        self.forget_lost(answer.id);
        true
    }

    /// Marks each picture lost whose `ENOENT` the loss limit ignored and whose window ended at
    /// `now`. Without this, the cell stays empty until a new placement brings a new `ENOENT`.
    pub fn release_held_losses(&mut self, now: Instant) {
        let due: Vec<u32> = self
            .held
            .iter()
            .filter(|&(_, &end)| end <= now)
            .map(|(&id, _)| id)
            .collect();
        for id in due {
            self.held.remove(&id);
            if self.store.contains(id) {
                self.forget_lost(id);
            }
        }
    }

    /// True when the terminal lost a picture since the last flush. The app then writes a frame
    /// soon, so the lost picture shows again.
    pub const fn has_lost(&self) -> bool {
        self.lost_any
    }

    /// `Screen::flush` into an update that the app opened, after its own
    /// writes. The pixel bytes of the update count for the byte cap.
    pub fn write_frame<'a, W: Write + ?Sized>(
        &mut self,
        update: &mut Update<'_, W>,
        backend: KittyBackend,
        frame: &mut Placements,
        deleted: &[u32],
        pixels: impl FnMut(u32) -> Option<Pixels<'a>>,
    ) -> io::Result<()> {
        let frame = std::mem::take(frame);
        self.lost_any = false;
        for &id in deleted {
            let stored = self.forget(id);
            let maybe_stored = self.maybe_stored.remove(&id);
            if stored || maybe_stored {
                protocol::delete_image_to(update, id)?;
            }
        }
        let cap = update.pass_bytes.unwrap_or(self.pass_bytes);
        let (mut steps, retired) = self.plan(&frame, update.pixel_bytes(), cap, pixels);
        let gone = self.number(&mut steps);
        for (id, placement) in gone {
            self.placed.remove(&(id, placement));
            if !retired.contains(&id) {
                protocol::delete_placement_to(update, id, placement)?;
            }
        }
        for step in steps {
            self.write_step(update, backend, step)?;
        }
        for &id in &retired {
            if self.forget(id) {
                protocol::delete_image_to(update, id)?;
            }
        }
        if self.store.bytes() <= self.store.budget {
            return Ok(());
        }
        let keep: HashSet<u32> = frame
            .places
            .iter()
            .flat_map(|place| [Some(place.id), place.earlier])
            .flatten()
            .chain(frame.kept.iter().copied())
            .collect();
        for id in self.store.over_budget(&keep) {
            self.placed.retain(|&(placed, _), _| placed != id);
            self.resend.remove(&id);
            protocol::delete_image_to(update, id)?;
        }
        Ok(())
    }

    /// Decides for each place which picture shows there and which pixels go out, in the order
    /// of the frame, within the byte cap. Returns the steps without placement ids and the
    /// earlier forms that go.
    fn plan<'a>(
        &mut self,
        frame: &Placements,
        mut written: usize,
        cap: usize,
        mut pixels: impl FnMut(u32) -> Option<Pixels<'a>>,
    ) -> (Vec<Step<'a>>, BTreeSet<u32>) {
        self.waiting.clear();
        let mut sent = HashSet::new();
        let mut steps = Vec::with_capacity(frame.places.len());
        let mut retired = BTreeSet::new();
        for place in &frame.places {
            let shown = Shown {
                cells: place.cells,
                layer: place.layer,
            };
            let step = |id, send| Step {
                id,
                shown,
                placement: 0,
                send,
                write: false,
            };
            let needs_send = !self.store.contains(place.id) || self.resend.contains(&place.id);
            if sent.contains(&place.id) || !needs_send {
                steps.push(step(place.id, None));
            } else if written >= cap {
                if !self.waiting.contains(&place.id) {
                    self.waiting.push(place.id);
                }
                if self.store.contains(place.id) {
                    steps.push(step(place.id, None));
                } else if let Some((earlier, shown)) = self.stand_in(place, shown) {
                    steps.push(Step {
                        shown,
                        ..step(earlier, None)
                    });
                }
                continue;
            } else if let Some(data) = pixels(place.id) {
                written = written.saturating_add(data.data.len());
                sent.insert(place.id);
                steps.push(step(place.id, Some(data)));
            } else {
                if let Some((earlier, shown)) = self.stand_in(place, shown) {
                    steps.push(Step {
                        shown,
                        ..step(earlier, None)
                    });
                }
                continue;
            }
            retired.extend(place.earlier.filter(|&earlier| earlier != place.id));
        }
        // An earlier form that still shows in another place stays.
        retired.retain(|id| !steps.iter().any(|step| step.id == *id));
        (steps, retired)
    }

    /// The earlier form of `place` that keeps the place while the picture waits, when the
    /// terminal stores it. It stays on the cells of its placement: a preview can have other cells
    /// than its sharp picture.
    fn stand_in(&self, place: &Place, shown: Shown) -> Option<(u32, Shown)> {
        let earlier = place.earlier.filter(|&id| self.store.contains(id))?;
        let placed = self
            .placed
            .range((earlier, 0)..=(earlier, u32::MAX))
            .next()
            .map(|(_, &placed)| placed);
        Some((earlier, placed.unwrap_or(shown)))
    }

    /// Gives each step its placement id and decides whether its placement goes out. A place that
    /// the terminal already shows keeps its placement id, a moved place takes a free one of its
    /// picture. Returns the placements that the frame no longer has.
    fn number(&self, steps: &mut [Step<'_>]) -> Vec<(u32, u32)> {
        let mut free: BTreeMap<u32, Vec<(u32, Shown)>> = BTreeMap::new();
        for (&(id, placement), &shown) in &self.placed {
            free.entry(id).or_default().push((placement, shown));
        }
        let new_pixels: HashSet<u32> = steps
            .iter()
            .filter(|step| step.send.is_some())
            .map(|step| step.id)
            .collect();
        let mut taken: HashSet<(u32, u32)> = HashSet::new();
        // First the unchanged places, so a move elsewhere does not take their placement id.
        for step in steps.iter_mut() {
            let unchanged = free.get(&step.id).and_then(|placements| {
                placements.iter().find(|&&(placement, shown)| {
                    shown == step.shown && !taken.contains(&(step.id, placement))
                })
            });
            if let Some(&(placement, _)) = unchanged {
                taken.insert((step.id, placement));
                step.placement = placement;
                step.write = self.placing == Placing::EveryFrame || new_pixels.contains(&step.id);
            }
        }
        for step in steps.iter_mut().filter(|step| step.placement == 0) {
            let reused = free.get(&step.id).and_then(|placements| {
                placements
                    .iter()
                    .map(|&(placement, _)| placement)
                    .find(|&placement| !taken.contains(&(step.id, placement)))
            });
            let placement = reused.unwrap_or_else(|| {
                (1..=u32::MAX)
                    .find(|&placement| !taken.contains(&(step.id, placement)))
                    .unwrap_or(u32::MAX)
            });
            taken.insert((step.id, placement));
            step.placement = placement;
            step.write = true;
        }
        self.placed
            .keys()
            .filter(|key| !taken.contains(key))
            .copied()
            .collect()
    }

    /// Writes one step: the cursor, then the transfer with its place (`a=T`) or the place alone
    /// (`a=p`). Records the picture and the placement.
    fn write_step<W: Write + ?Sized>(
        &mut self,
        update: &mut Update<'_, W>,
        backend: KittyBackend,
        step: Step<'_>,
    ) -> io::Result<()> {
        let Step {
            id,
            shown,
            placement,
            send,
            write,
        } = step;
        let cells = shown.cells;
        let below_backgrounds = shown.layer == Layer::UnderCells;
        if write {
            write!(update, "\x1b[{};{}H", cells.y + 1, cells.x + 1)?;
        }
        if let Some(data) = send {
            backend.transmit(
                update,
                data,
                &DisplayOptions {
                    id: Some(id),
                    placement: Some(placement),
                    cell_box: Some((cells.width, cells.height)),
                    below_backgrounds,
                    ..DisplayOptions::default()
                },
            )?;
            update.add_pixel_bytes(data.data.len());
            self.record(id, stored_size(data));
        } else if write {
            protocol::place_as_to(
                update,
                id,
                placement,
                (cells.width, cells.height),
                below_backgrounds,
                self.self_check,
            )?;
        }
        self.store.touch(id);
        self.placed.insert((id, placement), shown);
        Ok(())
    }

    /// Records the picture `id` of `bytes` pixel bytes as the newest one. The app calls it for a
    /// picture that it sent itself, for example an animation.
    pub fn record(&mut self, id: u32, bytes: usize) {
        self.store.insert(id, bytes);
        self.maybe_stored.remove(&id);
        self.resend.remove(&id);
    }

    /// True when the terminal stores the picture `id`.
    pub fn stores(&self, id: u32) -> bool {
        self.store.contains(id)
    }

    /// The pixel bytes of the stored picture `id`, or 0.
    pub fn bytes_of(&self, id: u32) -> usize {
        self.store.bytes_of(id)
    }

    /// The pixel bytes that the terminal stores at most.
    pub const fn budget(&self) -> usize {
        self.store.budget
    }

    /// True when the placements ask for the error answers of the terminal.
    #[cfg(any(test, feature = "test-support"))]
    pub const fn self_checks(&self) -> bool {
        self.self_check
    }

    /// The pictures that the terminal stores now.
    #[cfg(any(test, feature = "test-support"))]
    pub fn stored_count(&self) -> usize {
        self.store.len()
    }

    /// The pictures of the last flush that wait for a later pass, in the order of the frame.
    pub fn waiting(&self) -> &[u32] {
        &self.waiting
    }

    /// Deletes every picture of the terminal with one `d=A`, also the pictures that the app
    /// wrote without the engine.
    pub fn wipe<W: Write + ?Sized>(&mut self, out: &mut W) -> io::Result<()> {
        protocol::delete_all_to(out)?;
        self.forget_all();
        Ok(())
    }

    /// The app deleted every placement itself (`d=a`). The next flush places every picture
    /// again.
    pub fn forget_placements(&mut self) {
        self.placed.clear();
    }

    /// Forgets every picture and every placement.
    fn forget_all(&mut self) {
        self.store.clear();
        self.maybe_stored.clear();
        self.resend.clear();
        self.placed.clear();
        self.waiting.clear();
    }

    /// Forgets the picture `id` that the terminal lost, so the next flush sends it again.
    fn forget_lost(&mut self, id: u32) {
        self.forget(id);
        self.lost_any = true;
    }

    /// Removes the picture `id` and its placements from the record. Returns `false` when the
    /// terminal did not store it.
    fn forget(&mut self, id: u32) -> bool {
        self.placed.retain(|&(placed, _), _| placed != id);
        self.resend.remove(&id);
        self.held.remove(&id);
        let stored = self.store.contains(id);
        self.store.remove(id);
        stored
    }
}

/// The settings and the events for apps with other needs: the placement policy, the byte cap,
/// the own update, the resize, the clear, the focus gain and the quit. Each part has its own
/// feature, so an app builds only the parts that it calls. The tests build every part.
impl Screen {
    /// The same screen with the placement policy `placing`.
    #[must_use]
    pub const fn with_placing(mut self, placing: Placing) -> Self {
        self.placing = placing;
        self
    }

    /// The same screen with a byte cap of `bytes` new pixel bytes per flush.
    #[must_use]
    pub const fn with_pass_bytes(mut self, bytes: usize) -> Self {
        self.pass_bytes = bytes;
        self
    }

    /// Writes the frame in one synchronized update: the deletes of `deleted`, the removed
    /// placements, the new pictures up to the byte cap, the changed placements and the deletes
    /// of the budget. `pixels` gives the data of a picture that the terminal does not store. A
    /// picture without data gets no placement. `frame` is empty after the call.
    pub fn flush<'a>(
        &mut self,
        out: &mut impl Write,
        backend: KittyBackend,
        frame: &mut Placements,
        deleted: &[u32],
        pixels: impl FnMut(u32) -> Option<Pixels<'a>>,
    ) -> io::Result<()> {
        let mut update = Update::new(out);
        self.write_frame(&mut update, backend, frame, deleted, pixels)?;
        update.finish()
    }

    /// The pixel bytes that the terminal stores now.
    pub fn stored_bytes(&self) -> usize {
        self.store.bytes()
    }

    /// A resize to a new size clears the screen, and the clear deletes every picture. The next
    /// flushes send and place every picture again.
    pub fn resized(&mut self, columns: u16, rows: u16) {
        if self.size.replace((columns, rows)) != Some((columns, rows)) {
            self.cleared();
        }
    }

    /// The app cleared the screen, for example after a program ran in the terminal. The next
    /// flushes send and place every picture again. The terminal can still hold the pictures, so
    /// their ids move to `maybe_stored`, and a later delete still reaches them.
    pub fn cleared(&mut self) {
        let ids: Vec<u32> = self.store.ids().collect();
        self.maybe_stored.extend(ids);
        self.store.clear();
        self.resend.clear();
        self.placed.clear();
        self.waiting.clear();
    }

    /// The terminal has the focus again. A terminal or a multiplexer can drop pictures while
    /// another app has the focus, so each picture sends its pixels again at its next placement.
    /// Nothing cleared the screen: the pictures still count for the budget, and a placement that
    /// the next frame does not use still gets its delete.
    pub fn focus_gained(&mut self) {
        self.resend.extend(self.store.ids());
    }

    /// Deletes every stored picture, so the terminal frees its memory when the app quits.
    pub fn delete_all(&mut self, out: &mut impl Write) -> io::Result<()> {
        let mut ids: BTreeSet<u32> = self.store.ids().collect();
        ids.append(&mut self.maybe_stored);
        let mut update = Update::new(out);
        for id in ids {
            protocol::delete_image_to(&mut update, id)?;
        }
        self.forget_all();
        update.finish()
    }

    /// Deletes every placement with one `d=a`, also the placements that the app wrote without
    /// the engine. The terminal keeps the pixels, so the next flush only places.
    pub fn unplace_all<W: Write + ?Sized>(&mut self, out: &mut W) -> io::Result<()> {
        protocol::clear_placements_to(out)?;
        self.forget_placements();
        Ok(())
    }

    /// The terminal lost the picture `id`, for example after an error answer. The next flush
    /// that places it sends it again.
    pub fn lost(&mut self, id: u32) {
        self.forget_lost(id);
    }
}

/// The bytes that the terminal stores for `picture`: 4 per pixel, also for a picture that goes
/// out as RGB, so the count is an upper limit.
fn stored_size(picture: Pixels<'_>) -> usize {
    let pixels = u64::from(picture.width) * u64::from(picture.height) * 4;
    usize::try_from(pixels).unwrap_or(usize::MAX)
}

#[cfg(test)]
mod tests;
