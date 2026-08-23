#!/usr/bin/env python3
import io
import os
import subprocess
from PIL import Image, ImageDraw, ImageFont, ImageFilter

HERE = os.path.dirname(os.path.abspath(__file__))


def font(size):
    for path in [
        "/System/Library/Fonts/HelveticaNeue.ttc",
        "/System/Library/Fonts/Helvetica.ttc",
        "/System/Library/Fonts/Supplemental/Arial.ttf",
    ]:
        if os.path.exists(path):
            try:
                return ImageFont.truetype(path, size)
            except Exception:
                pass
    return ImageFont.load_default()


def flash(size):
    png = subprocess.check_output([
        "rsvg-convert", "-w", str(size), "-h", str(size),
        os.path.join(HERE, "favicon.svg"),
    ])
    return Image.open(io.BytesIO(png)).convert("RGBA")


def vgradient(w, h, top, bot):
    col = Image.new("RGBA", (1, h))
    for y in range(h):
        t = y / max(h - 1, 1)
        col.putpixel((0, y), tuple(int(top[i] + (bot[i] - top[i]) * t) for i in range(3)) + (255,))
    return col.resize((w, h))


def make_icon():
    s = 1024
    icon = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    tile = vgradient(s, s, (30, 31, 42), (14, 15, 22))
    mask = Image.new("L", (s, s), 0)
    ImageDraw.Draw(mask).rounded_rectangle([96, 96, s - 96, s - 96], radius=205, fill=255)
    icon.paste(tile, (0, 0), mask)

    glow = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([300, 300, 724, 724], fill=(134, 59, 255, 120))
    icon = Image.alpha_composite(icon, glow.filter(ImageFilter.GaussianBlur(90)))

    bolt = flash(560)
    icon.alpha_composite(bolt, ((s - 560) // 2, (s - 560) // 2))
    icon.save(os.path.join(HERE, "icon_master.png"))


def make_background():
    w, h = 540, 380
    bg = Image.new("RGBA", (w, h), (10, 11, 16, 255))
    glow = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    ImageDraw.Draw(glow).ellipse([w / 2 - 170, -130, w / 2 + 170, 180], fill=(120, 127, 250, 70))
    bg = Image.alpha_composite(bg, glow.filter(ImageFilter.GaussianBlur(70)))

    d = ImageDraw.Draw(bg)
    for text, y, size, col in [
        ("ss-bridge", 40, 30, (244, 245, 255, 255)),
        ("Arraste para a pasta Aplicativos", 78, 14, (138, 143, 158, 255)),
    ]:
        f = font(size)
        box = d.textbbox((0, 0), text, font=f)
        d.text(((w - (box[2] - box[0])) / 2 - box[0], y), text, font=f, fill=col)

    y = 192
    d.line([(212, y), (326, y)], fill=(150, 154, 180, 235), width=3)
    d.polygon([(326, y - 8), (344, y), (326, y + 8)], fill=(150, 154, 180, 235))

    # Finder draws the icon labels in dark text; give them light plates so they
    # stay legible on the dark background.
    plate = Image.new("RGBA", (w, h), (0, 0, 0, 0))
    pd = ImageDraw.Draw(plate)
    for cx in (140, 400):
        pd.rounded_rectangle([cx - 62, 250, cx + 62, 273], radius=11, fill=(236, 238, 250, 235))
    bg = Image.alpha_composite(bg, plate)
    bg.convert("RGB").save(os.path.join(HERE, "dmg-background.png"))


make_icon()
make_background()
print("assets written")
