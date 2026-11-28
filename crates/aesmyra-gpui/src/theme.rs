//! Farben, Maße und Schriften der Oberfläche.
//!
//! Die Palette stammt unverändert aus `ui/aesmyra.css`: dunkles Tintenpapier,
//! warmer Text, ein violetter Akzent als HSL-Tripel. Der Akzent bleibt
//! veränderbar (Farbton und Sättigung), damit die Einstellungen ihn drehen
//! können, ohne dass jede Regel angefasst wird.

use gpui::{rgb, Rgba};

pub const INK: u32 = 0x08070b;
pub const FG: u32 = 0xece7f4;
pub const BODY: u32 = 0xc9c2d6;
pub const DIM: u32 = 0x7e7690;
pub const FAINT: u32 = 0x5b5468;
pub const RULE: u32 = 0x1b1822;
pub const RULE_2: u32 = 0x272231;
pub const ERR: u32 = 0xff8b9c;

/// Ein HSL-Tripel, wie es die Einstellungen verstellen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsl {
    pub h: f32,
    pub s: f32,
    pub l: f32,
}

impl Hsl {
    pub fn to_rgba(self) -> Rgba {
        let h = self.h.rem_euclid(360.0) / 60.0;
        let c = (1.0 - (2.0 * self.l - 1.0).abs()) * self.s;
        let x = c * (1.0 - ((h % 2.0) - 1.0).abs());
        let m = self.l - c / 2.0;
        let (r, g, b) = match h as u32 {
            0 => (c, x, 0.0),
            1 => (x, c, 0.0),
            2 => (0.0, c, x),
            3 => (0.0, x, c),
            4 => (x, 0.0, c),
            _ => (c, 0.0, x),
        };
        Rgba {
            r: r + m,
            g: g + m,
            b: b + m,
            a: 1.0,
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub dark: bool,
    pub accent: Hsl,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            dark: true,
            accent: Hsl { h: 258.0, s: 0.92, l: 0.76 },
        }
    }
}

impl Theme {
    pub fn ink(&self) -> Rgba {
        if self.dark { rgb(INK) } else { rgb(0xf6f4f9) }
    }
    pub fn fg(&self) -> Rgba {
        if self.dark { rgb(FG) } else { rgb(0x14111c) }
    }
    pub fn body(&self) -> Rgba {
        if self.dark { rgb(BODY) } else { rgb(0x3b3548) }
    }
    pub fn dim(&self) -> Rgba {
        if self.dark { rgb(DIM) } else { rgb(0x6a6379) }
    }
    pub fn faint(&self) -> Rgba {
        if self.dark { rgb(FAINT) } else { rgb(0x8d8699) }
    }
    pub fn rule(&self) -> Rgba {
        if self.dark { rgb(RULE) } else { rgb(0xe4e0ec) }
    }
    pub fn rule2(&self) -> Rgba {
        if self.dark { rgb(RULE_2) } else { rgb(0xd6d1e0) }
    }
    pub fn acc(&self) -> Rgba {
        self.accent.to_rgba()
    }
    pub fn acc_soft(&self, a: f32) -> Rgba {
        let mut c = self.accent.to_rgba();
        c.a = a;
        c
    }
}

/// Schriftgrößen des Leseflusses.
pub const BODY_SIZE: f32 = 17.0;
pub const BODY_LINE: f32 = 29.0;
pub const SERIF: &str = "Newsreader";
pub const MONO: &str = "JetBrains Mono";
