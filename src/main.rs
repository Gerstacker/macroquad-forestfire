use macroquad::prelude::*;

use macroquad::ui::{hash, root_ui, widgets};
use std::process::exit;

struct DebounceToggle<F: Fn() -> bool>(F, usize);

impl<F: Fn() -> bool> DebounceToggle<F> {
    fn new(f: F) -> DebounceToggle<F> {
        DebounceToggle(f, 0)
    }
    fn get(&mut self) -> bool {
        let DebounceToggle(f, ref mut state) = self;

        *state = match (*state, f()) {
            (0, true) => 1,
            (1, false) => 2,
            (2, true) => 3,
            (3, false) => 0,
            (_, _) => *state,
        };

        *state == 2
    }
}

struct PoissonProcess(f32);

impl PoissonProcess {
    fn new() -> PoissonProcess {
        PoissonProcess(0.0)
    }
    fn draw(&mut self, avgper: f32) -> usize {
        let PoissonProcess(ref mut acc) = self;

        let ur: f32 = rand::gen_range(f32::EPSILON, 1.);
        let er = -avgper * ur.ln();
        let newacc = *acc + er;
        let faf = newacc.floor();
        *acc = newacc - faf;
        faf as usize
    }
}

struct Fire(usize, usize, usize);

struct CellField {
    arr: Vec<u64>,
    ystride: usize,
}

impl CellField {
    fn new(w: usize, h: usize) -> CellField {
        let nx = (w + 7) / 8;
        let ny = (h + 7) / 8;
        CellField {
            arr: vec![0; nx * ny],
            ystride: nx,
        }
    }
    fn indices(&self, x: usize, y: usize) -> (usize, usize) {
        let (ox, ix) = (x / 8, x % 8);
        let (oy, iy) = (y / 8, y % 8);
        let s = iy * 8 + ix;
        return (oy * self.ystride + ox, s);
    }
    fn get(&self, x: usize, y: usize) -> bool {
        let (off, s) = self.indices(x, y);
        return (self.arr[off] & (1 << s)) != 0;
    }
    fn set(&mut self, x: usize, y: usize) {
        let (off, s) = self.indices(x, y);
        self.arr[off] |= 1 << s;
    }
    fn clr(&mut self, x: usize, y: usize) {
        let (off, s) = self.indices(x, y);
        self.arr[off] &= !(1 << s);
    }
}

fn conf() -> Conf {
    Conf {
        window_title: String::from("Forest Fires: <space> or double touch for controls"),
        high_dpi: false,
        ..Default::default()
    }
}

#[macroquad::main(conf)]
async fn main() {
    let fireprob: f32 = 1e-6;
    let treeprob: f32 = 1e-3;

    let mut logfireprob: f32 = fireprob.log10();
    let mut logtreeprob: f32 = treeprob.log10();
    let mut colorspeed: f32 = 5.;
    let mut firemaxage: f32 = 10.;
    let mut eightconn: bool = false;

    let w = screen_width() as usize;
    let h = screen_height() as usize;

    let mut cellfield = CellField::new(w, h);
    let mut fires: Vec<Fire> = Vec::new();

    let mut image = Image::gen_image_color(w as u16, h as u16, BLACK);

    let alive_color = Color::new(0.0, 0.5, 0.0, 1.0);

    for y in 0..h {
        for x in 0..w {
            if rand::gen_range(0, 4 as usize) == 0 {
                cellfield.set(x, y);
                image.set_pixel(x as u32, y as u32, alive_color);
            }
        }
    }
    let texture = Texture2D::from_image(&image);

    let ngh: [[i32; 2]; 8] = [
        [-1, 0],
        [1, 0],
        [0, -1],
        [0, 1],
        [-1, -1],
        [-1, 1],
        [1, -1],
        [1, 1],
    ];

    let mut frno: usize = 0;

    let mut showpopup = DebounceToggle::new(|| is_key_down(KeyCode::Space) || touches().len() == 2);
    let mut recording: bool = false;
    let mut rfrm: usize = 0;
    let mut recskip: f32 = 1.;

    let mut colorphase: f32 = 0.;

    let mut fireproc = PoissonProcess::new();
    let mut treeproc = PoissonProcess::new();

    simulate_mouse_with_touch(false);

    loop {
        clear_background(BLACK);

        if is_key_down(KeyCode::Q) {
            exit(0);
        }

        if showpopup.get() {
            widgets::Window::new(hash!(), vec2(100., 100.), vec2(300., 200.))
                .label(&format!("Step {}", frno))
                .ui(&mut *root_ui(), |ui| {
                    ui.slider(hash!(), "logfireprob", -10f32..-5f32, &mut logfireprob);
                    ui.slider(hash!(), "logtreeprob", -10f32..-2f32, &mut logtreeprob);
                    ui.slider(hash!(), "colorspeed", 0f32..10f32, &mut colorspeed);
                    ui.slider(hash!(), "firemaxage", 0f32..20f32, &mut firemaxage);
                    ui.checkbox(hash!(), "8-connected", &mut eightconn);

                    ui.tree_node(hash!(), "Save PNG", |ui| {
                        let btext: String = match recording {
                            false => "Start Recording".to_string(),
                            true => format!("Recording {}", rfrm).to_string(),
                        };
                        if ui.button(None, btext) {
                            rfrm = 0;
                            recording = !recording;
                        }
                        ui.slider(hash!(), "recskip", 1f32..10f32, &mut recskip);
                    });
                });
        }

        let w = image.width();
        let h = image.height();
        let numngh: usize = if eightconn { 8 } else { 4 };

        let mut newfires: Vec<Fire> = Vec::new();

        // propagate new fires, age out old fires
        for Fire(x, y, age) in &fires {
            if *age < firemaxage.floor() as usize {
                newfires.push(Fire(*x, *y, *age + 1));
            } else {
                image.set_pixel(*x as u32, *y as u32, BLACK);
            }
            for j in 0..numngh {
                let nx = *x as i32 + ngh[j][0];
                let ny = *y as i32 + ngh[j][1];
                if nx >= 0 && nx < w as i32 && ny >= 0 && ny < h as i32 {
                    let cx = nx as usize;
                    let cy = ny as usize;
                    if cellfield.get(cx, cy) {
                        newfires.push(Fire(cx, cy, 0));
                        cellfield.clr(cx, cy);
                    }
                }
            }
        }

        // spontaneous fires
        for _ in 0..fireproc.draw(10f32.powf(logfireprob) * h as f32 * w as f32) {
            newfires.push(Fire(rand::gen_range(0, w), rand::gen_range(0, h), 0));
        }

        if is_mouse_button_down(MouseButton::Left) {
            let (mouse_x, mouse_y) = mouse_position();
            let mx = clamp(mouse_x as usize, 0, w - 1);
            let my = clamp(mouse_y as usize, 0, h - 1);
            newfires.push(Fire(mx, my, 0));
        }

        if touches().len() == 1 {
            let touchpos = touches()[0].position;

            let mx = clamp(touchpos.x as usize, 0, w - 1);
            let my = clamp(touchpos.y as usize, 0, h - 1);
            newfires.push(Fire(mx, my, 0));
        }

        // new trees
        colorphase += colorspeed * 6.28 / 10000.;
        let g = colorphase.cos().abs();
        let b = colorphase.sin().abs();
        for _ in 0..treeproc.draw(10f32.powf(logtreeprob) * h as f32 * w as f32) {
            let x = rand::gen_range(0, w);
            let y = rand::gen_range(0, h);
            if !cellfield.get(x, y) {
                image.set_pixel(x as u32, y as u32, Color::new(0.0, g, b, 1.0));
            }
            cellfield.set(x, y);
        }

        for Fire(x, y, age) in &newfires {
            let grn: f32 = *age as f32 / firemaxage;
            image.set_pixel(*x as u32, *y as u32, Color::new(1., grn, 0., 1.0));
        }

        if false {
            newfires.sort_by(|Fire(x1, y1, _), Fire(x2, y2, _)| {
                cellfield
                    .indices(*x2, *y2)
                    .0
                    .cmp(&cellfield.indices(*x1, *y1).0)
            });
        }

        fires = newfires;

        texture.update(&image);

        draw_texture(texture, 0., 0., WHITE);

        if recording && frno % recskip.floor() as usize == 0 {
            image.export_png(format!("frm{:05}.png", rfrm).as_str());
            rfrm += 1;
        }

        frno = frno + 1;
        next_frame().await
    }
}
