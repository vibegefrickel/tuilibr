use super::{LoopLimits, LoopOutcome, LoopPlayer, LoopResult, LoopTicket, run_caught};
use crate::pictures::jobs::thumbnail_cache::GridPicture;
use crate::pictures::kitty::screen::{Placements, Screen, Update};
use crate::pictures::kitty::{KittyBackend, Loops, Transfer};
use crate::pictures::motion::clock::LoopClock;
use crate::pictures::motion::loop_frames::LoopFrames;
use image::{Rgba, RgbaImage};
use ratatui::layout::Rect;
use std::io;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// The loop ids of the tests.
const IDS: std::ops::Range<u32> = 1000..2000;

/// The time that each test frame shows: 4 ticks.
const FRAME: Duration = Duration::from_millis(333);

/// A player with `limits` on a clock that starts at the returned instant.
fn player(limits: LoopLimits) -> (LoopPlayer<u32>, Instant) {
    let epoch = Instant::now();
    let player = LoopPlayer::new(IDS, limits).with_clock(LoopClock::starting_at(epoch));
    (player, epoch)
}

/// `count` different frames of 2x2 pixels: 16 bytes each.
fn frames(count: u8) -> Vec<GridPicture> {
    (0..count)
        .map(|value| RgbaImage::from_pixel(2, 2, Rgba([value, 0, 0, 255])).into())
        .collect()
}

/// The result of `ticket` with a loop of `count` frames.
fn loop_of(ticket: LoopTicket<u32>, count: u8) -> LoopResult<u32> {
    ticket.finish(LoopOutcome::Frames(
        frames(count),
        vec![FRAME; usize::from(count)],
    ))
}

/// A frame at `now` that shows `keys` in this order, each playing.
fn show(player: &mut LoopPlayer<u32>, keys: &[u32], now: Instant) {
    player.begin_frame(now);
    for &key in keys {
        player.show(key, true);
    }
}

/// The tickets of the jobs that start now.
fn start(player: &mut LoopPlayer<u32>) -> Vec<LoopTicket<u32>> {
    let mut tickets = Vec::new();
    player.start_jobs(|ticket| {
        tickets.push(ticket);
        true
    });
    tickets
}

/// The keys of `tickets`.
fn keys(tickets: &[LoopTicket<u32>]) -> Vec<u32> {
    tickets.iter().map(|ticket| ticket.key).collect()
}

fn backend(loops: Loops) -> KittyBackend {
    KittyBackend::new(Transfer::Direct).with_loops(loops)
}

/// One flush of the shown `keys`, as an app writes it: the loop frames, the stale deletes, the
/// placements, each key on its own row, in one update. Returns the bytes as text.
fn flush(player: &mut LoopPlayer<u32>, screen: &mut Screen, loops: Loops, keys: &[u32]) -> String {
    let mut out = Vec::new();
    let mut update = Update::new(&mut out);
    let mut frame = Placements::default();
    player
        .send_frames(&mut update, screen, backend(loops), &mut frame, 0)
        .unwrap();
    let stale = player.take_stale();
    for (row, key) in keys.iter().enumerate() {
        let row = u16::try_from(row).unwrap();
        player.place(key, screen, &mut frame, |_| Rect::new(0, row, 2, 1));
    }
    screen
        .write_frame(&mut update, backend(loops), &mut frame, &stale, |id| {
            player.pixels_of(id)
        })
        .unwrap();
    update.finish().unwrap();
    String::from_utf8_lossy(&out).into_owned()
}

/// A player whose key 1 holds a loop of 3 frames on `loops`, sent and placed at tick 0.
fn playing(loops: Loops) -> (LoopPlayer<u32>, Screen, Instant, String) {
    let (mut player, epoch) = player(LoopLimits::default());
    let mut screen = Screen::default();
    show(&mut player, &[1], epoch);
    let ticket = start(&mut player).pop().unwrap();
    player.ready(loop_of(ticket, 3), loops);
    let text = flush(&mut player, &mut screen, loops, &[1]);
    (player, screen, epoch, text)
}

#[test]
fn jobs_start_in_screen_order_up_to_max_jobs() {
    let (mut player, epoch) = player(LoopLimits::default());
    show(&mut player, &[5, 3, 1, 0, 2, 4], epoch);
    assert_eq!(keys(&start(&mut player)), vec![5, 3, 1, 0]);
    assert!(player.pending());
    assert!(
        start(&mut player).is_empty(),
        "4 jobs run, the fifth box waits"
    );
}

#[test]
fn max_loops_counts_loops_and_jobs() {
    let limits = LoopLimits {
        max_jobs: 4,
        max_loops: Some(2),
    };
    let (mut player, epoch) = player(limits);
    show(&mut player, &[1, 2, 3], epoch);
    let mut tickets = start(&mut player);
    assert_eq!(keys(&tickets), vec![1, 2]);
    let second = tickets.pop().unwrap();
    player.ready(loop_of(tickets.pop().unwrap(), 2), Loops::Ticks);
    assert!(
        start(&mut player).is_empty(),
        "1 loop and 1 job fill the cap"
    );
    player.ready(second.finish(LoopOutcome::Failed), Loops::Ticks);
    assert_eq!(keys(&start(&mut player)), vec![3]);
}

#[test]
fn without_max_loops_every_box_loops() {
    let (mut player, epoch) = player(LoopLimits::default());
    let shown: Vec<u32> = (0..12).collect();
    show(&mut player, &shown, epoch);
    for _ in 0..3 {
        for ticket in start(&mut player) {
            player.ready(loop_of(ticket, 2), Loops::Ticks);
        }
    }
    assert!(shown.iter().all(|key| player.ids_of(key).len() == 2));
}

#[test]
fn a_failed_spawn_marks_the_box_failed() {
    let (mut player, epoch) = player(LoopLimits::default());
    show(&mut player, &[1], epoch);
    player.start_jobs(|_| false);
    assert!(!player.pending());
    assert!(start(&mut player).is_empty(), "no second try");
    player.forget_where(|&key| key == 1);
    assert_eq!(keys(&start(&mut player)), vec![1], "a forget tries again");
}

#[test]
fn a_hidden_box_cancels_its_job_and_its_loop_leaves_the_terminal() {
    let (mut player, mut screen, epoch, _) = playing(Loops::Ticks);
    show(&mut player, &[1, 2], epoch);
    let job = start(&mut player).pop().unwrap();
    assert_eq!(job.key, 2);
    let ids = player.ids_of(&1).to_vec();
    assert_eq!(ids, vec![1000, 1001, 1002]);

    show(&mut player, &[3], epoch);
    player.drop_hidden();
    assert!(
        job.cancel().load(Ordering::Relaxed),
        "the job of box 2 stops"
    );
    assert!(player.ids_of(&1).is_empty());
    let text = flush(&mut player, &mut screen, Loops::Ticks, &[3]);
    for id in ids {
        assert!(text.contains(&format!("a=d,d=I,i={id}")), "{text}");
        assert!(!screen.stores(id));
    }
    player.ready(loop_of(job, 2), Loops::Ticks);
    assert!(player.ids_of(&2).is_empty(), "the late result goes away");
    assert!(!player.pending());
}

#[test]
fn a_result_of_an_old_job_goes_away() {
    let (mut player, epoch) = player(LoopLimits::default());
    show(&mut player, &[1], epoch);
    let old = start(&mut player).pop().unwrap();
    show(&mut player, &[], epoch);
    player.drop_hidden();
    show(&mut player, &[1], epoch);
    let new = start(&mut player).pop().unwrap();
    player.ready(old.finish(LoopOutcome::Cancelled), Loops::Ticks);
    assert!(player.pending(), "the new job of the same box runs");
    player.ready(loop_of(new, 2), Loops::Ticks);
    assert_eq!(player.ids_of(&1).len(), 2);

    // A clear starts a new generation: the flag of the new job stays its own.
    show(&mut player, &[2], epoch);
    let before = start(&mut player).pop().unwrap();
    player.clear();
    assert!(before.cancel().load(Ordering::Relaxed));
    let after = start(&mut player).pop().unwrap();
    player.ready(loop_of(before, 2), Loops::Ticks);
    assert!(player.ids_of(&2).is_empty());
    assert!(!after.cancel().load(Ordering::Relaxed));
    player.ready(loop_of(after, 2), Loops::Ticks);
    assert_eq!(player.ids_of(&2).len(), 2);
}

#[test]
fn fewer_than_two_frames_fail_and_a_cancel_leaves_the_box_free() {
    let (mut player, epoch) = player(LoopLimits::default());
    show(&mut player, &[0, 1, 2], epoch);
    let mut tickets = start(&mut player).into_iter();
    player.ready(loop_of(tickets.next().unwrap(), 0), Loops::Ticks);
    player.ready(loop_of(tickets.next().unwrap(), 1), Loops::Ticks);
    let cancelled = tickets.next().unwrap();
    player.ready(cancelled.finish(LoopOutcome::Cancelled), Loops::Ticks);
    assert!(!player.pending());
    assert!(player.ids_of(&0).is_empty() && player.ids_of(&1).is_empty());
    assert_eq!(keys(&start(&mut player)), vec![2], "only the cancelled box");
}

#[test]
fn a_loop_over_the_budget_goes_stale_and_tries_again_on_its_return() {
    let (mut player, epoch) = player(LoopLimits::default());
    // 3 frames of 16 bytes do not fit 40 bytes.
    let mut screen = Screen::with_budget(40);
    show(&mut player, &[1], epoch);
    let ticket = start(&mut player).pop().unwrap();
    player.ready(loop_of(ticket, 3), Loops::Ticks);
    let ids = player.ids_of(&1).to_vec();
    let mut out = Vec::new();
    let mut update = Update::new(&mut out);
    let mut frame = Placements::default();
    let graphics = backend(Loops::Ticks);
    player
        .send_frames(&mut update, &mut screen, graphics, &mut frame, 0)
        .unwrap();
    drop(update);
    assert!(out.is_empty(), "no frame goes out");
    assert_eq!(player.take_stale(), ids, "the ids of the loop go");
    assert!(start(&mut player).is_empty(), "the box keeps its still");
    assert!(!player.needs_send());

    show(&mut player, &[], epoch);
    player.drop_hidden();
    show(&mut player, &[1], epoch);
    assert_eq!(keys(&start(&mut player)), vec![1], "a return tries again");
}

#[test]
fn the_ticks_swap_placements_without_pixels() {
    let (mut player, mut screen, epoch, text) = playing(Loops::Ticks);
    assert_eq!(text.matches("a=t").count(), 3, "{text}");
    assert!(text.contains("a=p,q=2,C=1,i=1000"), "{text}");
    assert!(player.is_playing(&1));

    show(&mut player, &[1], epoch + FRAME + FRAME / 10);
    let text = flush(&mut player, &mut screen, Loops::Ticks, &[1]);
    assert!(!text.contains("a=t"), "{text}");
    assert!(text.contains("a=p,q=2,C=1,i=1001"), "{text}");
}

#[test]
fn the_wake_is_the_next_frame_change() {
    let (mut player, epoch) = player(LoopLimits::default());
    let mut screen = Screen::default();
    show(&mut player, &[1], epoch);
    assert_eq!(player.next_wake(epoch), None, "no loop yet");
    let ticket = start(&mut player).pop().unwrap();
    player.ready(loop_of(ticket, 3), Loops::Ticks);
    assert!(player.needs_send());
    assert_eq!(
        player.next_wake(epoch),
        Some(Duration::ZERO),
        "the loop waits"
    );

    flush(&mut player, &mut screen, Loops::Ticks, &[1]);
    assert!(!player.needs_send());
    // The loop starts at tick 0, and its first frame ends at tick 4.
    let change = LoopClock::starting_at(epoch).instant_of(4);
    let now = epoch + Duration::from_millis(100);
    assert_eq!(player.next_wake(now), Some(change - now));
    assert!(!player.due(now));
    assert!(player.due(change));
    assert_eq!(player.next_wake(change), Some(Duration::ZERO));
}

#[test]
fn the_animation_path_sends_one_image_and_needs_no_wake() {
    let (player, screen, epoch, text) = playing(Loops::Animation);
    assert_eq!(player.ids_of(&1), &[1000]);
    assert_eq!(text.matches("a=t").count(), 1, "{text}");
    assert_eq!(text.matches("a=f").count(), 2, "{text}");
    assert_eq!(text.matches("N=1").count(), 3, "{text}");
    assert!(text.contains("a=a,q=2,i=1000"), "{text}");
    assert!(screen.stores(1000));
    assert!(player.is_playing(&1));
    assert_eq!(player.next_wake(epoch + FRAME), None);
    assert!(!player.due(epoch + FRAME));
}

#[test]
fn pixels_of_gives_the_tick_frames_only() {
    let (player, ..) = playing(Loops::Ticks);
    let pixels = player.pixels_of(1001).unwrap();
    assert_eq!(pixels.data.first(), Some(&1), "the second frame");
    assert!(player.pixels_of(999).is_none());
    let (player, ..) = playing(Loops::Animation);
    assert!(player.pixels_of(1000).is_none());
}

#[test]
fn a_focus_gain_sends_the_loops_again() {
    let (mut player, mut screen, epoch, _) = playing(Loops::Ticks);
    screen.focus_gained();
    player.focus_gained(&mut screen);
    show(&mut player, &[1], epoch);
    let text = flush(&mut player, &mut screen, Loops::Ticks, &[1]);
    assert!(
        text.contains("a=T"),
        "the frame of the tick goes out: {text}"
    );

    let (mut player, mut screen, epoch, _) = playing(Loops::Animation);
    screen.focus_gained();
    player.focus_gained(&mut screen);
    assert!(!player.is_playing(&1));
    show(&mut player, &[1], epoch);
    let text = flush(&mut player, &mut screen, Loops::Animation, &[1]);
    assert_eq!(
        text.matches("a=f").count(),
        2,
        "the whole animation: {text}"
    );
    assert!(player.is_playing(&1));
}

#[test]
fn a_dimmed_box_keeps_its_loop_and_shows_its_still() {
    let (mut player, mut screen, epoch, _) = playing(Loops::Ticks);
    player.begin_frame(epoch);
    player.show(1, false);
    player.show(2, false);
    player.drop_hidden();
    assert!(start(&mut player).is_empty(), "a dimmed box starts no job");
    let text = flush(&mut player, &mut screen, Loops::Ticks, &[1]);
    assert!(text.contains("a=d,d=i,i=1000"), "{text}");
    assert!(!player.is_playing(&1));
    assert_eq!(player.ids_of(&1).len(), 3, "the loop stays");
    assert!(!player.needs_send());
    assert_eq!(player.next_wake(epoch), None);
}

#[test]
fn loops_wait_while_stills_wait() {
    let (mut player, epoch) = player(LoopLimits::default());
    let mut screen = Screen::default().with_pass_bytes(1);
    let still = RgbaImage::new(2, 2);
    let mut frame = Placements::default();
    frame.place(1, Rect::new(0, 0, 1, 1));
    frame.place(2, Rect::new(0, 1, 1, 1));
    let graphics = backend(Loops::Ticks);
    screen
        .flush(&mut Vec::new(), graphics, &mut frame, &[], |_| {
            Some((&still).into())
        })
        .unwrap();
    assert_eq!(screen.waiting(), &[2]);

    show(&mut player, &[7], epoch);
    let ticket = start(&mut player).pop().unwrap();
    player.ready(loop_of(ticket, 2), Loops::Ticks);
    let mut out = Vec::new();
    let mut update = Update::new(&mut out);
    player
        .send_frames(&mut update, &mut screen, graphics, &mut frame, 0)
        .unwrap();
    drop(update);
    assert!(out.is_empty());
    assert!(player.needs_send());
}

#[test]
fn forget_where_drops_the_matching_keys() {
    let (mut player, epoch) = player(LoopLimits::default());
    show(&mut player, &[1, 2, 3], epoch);
    let mut tickets = start(&mut player).into_iter();
    player.ready(loop_of(tickets.next().unwrap(), 2), Loops::Ticks);
    player.ready(loop_of(tickets.next().unwrap(), 2), Loops::Ticks);
    let third = tickets.next().unwrap();
    let ids = player.ids_of(&1).to_vec();
    player.forget_where(|&key| key != 2);
    assert!(third.cancel().load(Ordering::Relaxed));
    assert_eq!(player.take_stale(), ids);
    assert_eq!(player.ids_of(&2).len(), 2);
    assert_eq!(keys(&start(&mut player)), vec![1, 3]);
}

#[test]
fn forget_placements_places_again_without_pixels() {
    let (mut player, mut screen, epoch, _) = playing(Loops::Ticks);
    player.forget_placements();
    screen.forget_placements();
    assert!(!player.is_playing(&1));
    assert_eq!(player.next_wake(epoch), Some(Duration::ZERO));
    show(&mut player, &[1], epoch);
    let text = flush(&mut player, &mut screen, Loops::Ticks, &[1]);
    assert!(!text.contains("a=t"), "{text}");
    assert!(text.contains("a=p,q=2,C=1,i=1000"), "{text}");
    assert!(player.is_playing(&1));
}

#[test]
fn cancel_jobs_keeps_them_counted_and_a_drop_cancels_them() {
    let limits = LoopLimits {
        max_jobs: 1,
        max_loops: None,
    };
    let (mut player, epoch) = player(limits);
    show(&mut player, &[1, 2], epoch);
    let ticket = start(&mut player).pop().unwrap();
    player.cancel_jobs();
    assert!(ticket.cancel().load(Ordering::Relaxed));
    assert!(
        start(&mut player).is_empty(),
        "the stopped job holds its slot"
    );
    player.ready(ticket.finish(LoopOutcome::Cancelled), Loops::Ticks);
    let ticket = start(&mut player).pop().unwrap();
    assert_eq!(ticket.key, 1, "the cancelled box starts again first");
    drop(player);
    assert!(ticket.cancel().load(Ordering::Relaxed));
}

#[test]
fn outcomes_map_the_errors_and_a_panic_fails() {
    let loop_frames = |count| {
        Ok(LoopFrames {
            frames: vec![RgbaImage::new(4, 2); count],
            delays: vec![FRAME; count],
            shown: (8, 4),
        })
    };
    let mut seen = Vec::new();
    let outcome = LoopOutcome::of(loop_frames(2), |frame, shown| {
        seen.push(shown);
        frame.into()
    });
    assert!(matches!(&outcome, LoopOutcome::Frames(frames, _) if frames.len() == 2));
    assert_eq!(seen, vec![(8, 4); 2]);
    let shape = |frame: RgbaImage, _| GridPicture::from(frame);
    assert!(matches!(
        LoopOutcome::of(loop_frames(1), shape),
        LoopOutcome::Failed
    ));
    let interrupted = Err(io::Error::from(io::ErrorKind::Interrupted));
    assert!(matches!(
        LoopOutcome::of(interrupted, shape),
        LoopOutcome::Cancelled
    ));
    let broken = Err(io::Error::other("a broken file"));
    assert!(matches!(
        LoopOutcome::of(broken, shape),
        LoopOutcome::Failed
    ));
    assert_eq!(format!("{outcome:?}"), "Frames(2)");

    let caught = run_caught(|| panic!("a decode that panics"));
    assert!(matches!(caught, LoopOutcome::Failed));
    assert!(matches!(
        run_caught(|| LoopOutcome::Cancelled),
        LoopOutcome::Cancelled
    ));
}

#[test]
fn the_debug_output_holds_counts() {
    let (player, ..) = playing(Loops::Ticks);
    let text = format!("{player:?}");
    assert!(text.contains("loops: 1"), "{text}");
    assert!(!text.contains("frames"), "{text}");
}
