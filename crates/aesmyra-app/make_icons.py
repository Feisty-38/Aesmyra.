#!/usr/bin/env python3
"""Erzeugt die Aesmyra.-Symbole für Linux, Windows und Android."""

from pathlib import Path

from PIL import Image, ImageDraw

OUT = Path(__file__).resolve().parent
BASE = 1024


def make(size: int, pad_ratio: float = 0.0) -> Image.Image:
    ss = 4  # Übergröße zum Glätten
    s = BASE * ss
    img = Image.new("RGBA", (s, s), (8, 7, 11, 0))
    d = ImageDraw.Draw(img)

    # Hintergrund: dunkle Fläche mit rundem Rand
    r = int(s * 0.22)
    d.rounded_rectangle([0, 0, s - 1, s - 1], radius=r, fill=(8, 7, 11, 255))

    # Papier: helles Rechteck, leicht nach oben versetzt
    m = int(s * 0.24)
    top = int(s * 0.20)
    bottom = s - int(s * 0.20)
    paper = (236, 231, 244, 255)
    d.rounded_rectangle([m, top, s - m, bottom], radius=int(s * 0.012), fill=paper)

    # Purpurne Randlinie links (die Marke von Aesmyra.)
    d.rectangle([m, top, m + int(s * 0.016), bottom], fill=(124, 92, 245, 255))

    # Zeilen: kürzer werdend, wie ein Absatz
    line_h = int(s * 0.024)
    gap = int(s * 0.058)
    x0 = m + int(s * 0.075)
    widths = [0.52, 0.44, 0.50, 0.33]
    y = top + int(s * 0.10)
    for w in widths:
        d.rounded_rectangle(
            [x0, y, x0 + int(s * w), y + line_h],
            radius=line_h // 2,
            fill=(120, 112, 138, 255),
        )
        y += gap

    img = img.resize((size, size), Image.LANCZOS)
    return img


def main() -> None:
    icons = OUT / "icons"
    icons.mkdir(exist_ok=True)
    for name, size in {
        "32x32.png": 32,
        "128x128.png": 128,
        "128x128@2x.png": 256,
        "icon.png": 512,
    }.items():
        make(size).save(icons / name)

    make(1024).save(icons / "icon.ico", sizes=[(16, 16), (32, 32), (48, 48), (64, 64), (128, 128), (256, 256)])
    print("geschrieben:", ", ".join(sorted(p.name for p in icons.iterdir())))


if __name__ == "__main__":
    main()
