use std::hint::black_box;
use std::time::{Duration, Instant};

use selector::config::{Color, Config};
use selector::geometry::Rect;
use selector::render::{self, BYTES_PER_PIXEL, OPAQUE, Painted};

const FRAME_BUDGET: Duration = Duration::from_nanos(6_944_444);

const SIZES: [(u32, u32, &str); 3] = [
    (1920, 1080, "1080p"),
    (2560, 1440, "1440p"),
    (3840, 2160, "4K"),
];

fn config(border_width: u32, corner_radius: u32) -> Config {
    Config {
        fill: Color::rgba(0x4c, 0x9e, 0xd9, 0x40),
        border: Color::rgba(0x4c, 0x9e, 0xd9, 0xcc),
        border_width,
        corner_radius,
        ..Config::default()
    }
}

fn shown(rect: Rect, opacity: u8) -> Painted {
    Painted { rect, opacity }
}

fn centred(width: u32, height: u32, fraction: f64) -> Rect {
    let scale = fraction.sqrt();
    let (w, h) = (
        (width as f64 * scale) as u32,
        (height as f64 * scale) as u32,
    );
    Rect::new(((width - w) / 2) as i32, ((height - h) / 2) as i32, w, h)
}

struct Measurement {
    median: Duration,
    min: Duration,
    bytes: usize,
}

impl Measurement {
    fn report(&self, name: &str) {
        let median = self.median.as_secs_f64();
        let gb_per_s = self.bytes as f64 / median / 1e9;
        let budget = median / FRAME_BUDGET.as_secs_f64() * 100.0;

        println!(
            "  {name:<34} {:>9.3} ms median  {:>9.3} ms min  {gb_per_s:>6.2} GB/s  {budget:>6.1}% of a 144 Hz frame",
            median * 1e3,
            self.min.as_secs_f64() * 1e3,
        );
    }
}

fn measure(canvas: &mut [u8], mut body: impl FnMut(&mut [u8])) -> Measurement {
    const WARMUP: Duration = Duration::from_millis(100);
    const RUNTIME: Duration = Duration::from_millis(600);

    let warmup = Instant::now();
    while warmup.elapsed() < WARMUP {
        body(black_box(canvas));
    }

    let mut samples = Vec::new();
    let started = Instant::now();
    while started.elapsed() < RUNTIME {
        let iteration = Instant::now();
        body(black_box(canvas));
        samples.push(iteration.elapsed());
    }

    samples.sort_unstable();
    Measurement {
        median: samples[samples.len() / 2],
        min: samples[0],
        bytes: canvas.len(),
    }
}

fn main() {
    println!(
        "selector render benchmark — throughput is over the whole canvas, \
         so frames that only repaint what changed read far above memory bandwidth\n"
    );

    for (width, height, label) in SIZES {
        let mut canvas = vec![0u8; (width * height) as usize * BYTES_PER_PIXEL];
        println!("{label} ({width}x{height}, {} MiB)", canvas.len() >> 20);

        let square = config(1, 0);
        let rounded = config(2, 16);

        let scenarios: [(&str, Option<Rect>, &Config); 5] = [
            ("clear only (no selection)", None, &square),
            (
                "small rect (200x150)",
                Some(Rect::new(100, 100, 200, 150)),
                &square,
            ),
            (
                "30% of the surface",
                Some(centred(width, height, 0.30)),
                &square,
            ),
            (
                "full surface",
                Some(Rect::new(0, 0, width, height)),
                &square,
            ),
            (
                "30% rounded (r=16, border=2)",
                Some(centred(width, height, 0.30)),
                &rounded,
            ),
        ];

        let cold = [Rect::new(0, 0, width, height)];

        for (name, rect, cfg) in scenarios {
            let selection = rect.map(|rect| shown(rect, OPAQUE));
            measure(&mut canvas, |canvas| {
                render::draw(canvas, width, height, black_box(selection), &cold, cfg)
            })
            .report(name);
        }

        let base = centred(width, height, 0.30);

        for (name, cfg) in [
            ("30% nudged 1px (a drag frame)", &square),
            ("30% rounded, drag frame", &rounded),
        ] {
            render::draw(
                &mut canvas,
                width,
                height,
                Some(shown(base, OPAQUE)),
                &cold,
                cfg,
            );
            let mut nudge = 0i32;
            let mut previous = Some(shown(base, OPAQUE));
            measure(&mut canvas, |canvas| {
                nudge = (nudge + 1) % 2;
                let rect = Rect::new(base.x + nudge, base.y, base.width, base.height);
                let now = Some(shown(rect, OPAQUE));
                let stale = render::changes(previous, now, cfg, width, height);
                render::draw(canvas, width, height, black_box(now), &stale, cfg);
                previous = now;
            })
            .report(name);
        }

        let mut opacity = OPAQUE;
        measure(&mut canvas, |canvas| {
            let previous = Some(shown(base, opacity));
            opacity = opacity.wrapping_sub(1);
            let now = Some(shown(base, opacity));
            let stale = render::changes(previous, now, &rounded, width, height);
            render::draw(canvas, width, height, black_box(now), &stale, &rounded);
        })
        .report("30% rounded, fade frame");

        measure(&mut canvas, |canvas| {
            let previous = Some(shown(base, OPAQUE));
            let stale = render::changes(previous, None, &square, width, height);
            render::draw(canvas, width, height, black_box(None), &stale, &square)
        })
        .report("clearing a released selection");

        println!();
    }
}
