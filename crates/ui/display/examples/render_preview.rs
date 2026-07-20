// Standalone preview: render a frame via the real layout engine to a PNG so
// we can eyeball the TTF face before deploying to hardware.
use ui_display::layout::{LayoutConfig, LayoutEngine};

fn main() {
    let (w, h) = (250u32, 122u32);
    let mut buf = vec![0u8; (w as usize * h as usize).div_ceil(8)];
    let engine = LayoutEngine::new(LayoutConfig::default());
    // A few representative faces
    let faces = [
        ("(◕‿‿◕)", "awake.png"),
        ("(-_-')", "angry.png"),
        ("(⌐■_■)", "cool.png"),
        ("(♥‿‿♥)", "friend.png"),
    ];
    for (face, name) in faces {
        for b in buf.iter_mut() {
            *b = 0;
        }
        engine
            .draw_pwnagotchi_frame(
                &mut buf,
                w,
                h,
                6,
                3,
                "1234s",
                "pwnghost",
                "This is working!",
                face,
                2,
                7,
                3,
                150,
                "AUTO",
                None,
            )
            .unwrap();
        // Encode 1bpp buf -> grayscale PNG (bit=1 => black ink)
        let mut img = vec![255u8; (w * h) as usize];
        for y in 0..h {
            for x in 0..w {
                let idx = (y * w + x) as usize;
                if (buf[idx / 8] >> (idx % 8)) & 1 != 0 {
                    img[idx] = 0;
                }
            }
        }
        let path = format!("/tmp/{}", name);
        let file = std::fs::File::create(&path).unwrap();
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Grayscale);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header().unwrap().write_image_data(&img).unwrap();
        println!("wrote {}", path);
    }
}
