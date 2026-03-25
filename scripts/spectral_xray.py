#!/usr/bin/env python3

from __future__ import annotations

import argparse
import colorsys
from collections import deque
from dataclasses import dataclass
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw, ImageFont


@dataclass
class Hotspot:
    area: int
    mean_error: float
    peak_error: float
    bbox: tuple[int, int, int, int]
    centroid: tuple[int, int]


def load_rgb(path: Path) -> np.ndarray:
    return np.asarray(Image.open(path).convert("RGB"), dtype=np.float32)


def hsv_rainbow(level: np.ndarray) -> np.ndarray:
    level = np.clip(level, 0.0, 1.0)
    hue = (0.72 - 0.72 * (level ** 0.7)) % 1.0
    sat = np.clip(0.2 + level * 1.1, 0.0, 1.0)
    val = np.clip(0.08 + level * 1.1, 0.0, 1.0)

    flat = np.stack([hue, sat, val], axis=-1).reshape(-1, 3)
    rgb = np.empty_like(flat)
    for i, (h, s, v) in enumerate(flat):
        rgb[i] = colorsys.hsv_to_rgb(float(h), float(s), float(v))
    return (rgb.reshape(level.shape + (3,)) * 255.0).astype(np.uint8)


def signed_xray(delta: np.ndarray) -> np.ndarray:
    pos = np.clip(delta, 0.0, 255.0)
    neg = np.clip(-delta, 0.0, 255.0)
    magenta = np.stack([pos[..., 0], neg[..., 1] * 0.55, pos[..., 2]], axis=-1)
    cyan = np.stack([neg[..., 2], pos[..., 1], neg[..., 0]], axis=-1)
    base = np.clip(magenta + cyan, 0.0, 255.0)
    luma = np.mean(np.abs(delta), axis=2, keepdims=True) / 255.0
    glow = np.clip(0.25 + luma * 1.25, 0.0, 1.0)
    return np.clip(base * glow, 0.0, 255.0).astype(np.uint8)


def find_hotspots(error_map: np.ndarray, threshold: float, min_area: int) -> list[Hotspot]:
    h, w = error_map.shape
    mask = error_map >= threshold
    seen = np.zeros((h, w), dtype=bool)
    hotspots: list[Hotspot] = []

    for y in range(h):
        for x in range(w):
            if not mask[y, x] or seen[y, x]:
                continue

            q = deque([(x, y)])
            seen[y, x] = True
            area = 0
            total = 0.0
            peak = 0.0
            min_x = max_x = x
            min_y = max_y = y
            sum_x = 0
            sum_y = 0

            while q:
                cx, cy = q.popleft()
                value = float(error_map[cy, cx])
                area += 1
                total += value
                peak = max(peak, value)
                min_x = min(min_x, cx)
                max_x = max(max_x, cx)
                min_y = min(min_y, cy)
                max_y = max(max_y, cy)
                sum_x += cx
                sum_y += cy

                for nx, ny in (
                    (cx - 1, cy),
                    (cx + 1, cy),
                    (cx, cy - 1),
                    (cx, cy + 1),
                ):
                    if 0 <= nx < w and 0 <= ny < h and mask[ny, nx] and not seen[ny, nx]:
                        seen[ny, nx] = True
                        q.append((nx, ny))

            if area < min_area:
                continue

            hotspots.append(
                Hotspot(
                    area=area,
                    mean_error=total / area,
                    peak_error=peak,
                    bbox=(min_x, min_y, max_x + 1, max_y + 1),
                    centroid=(round(sum_x / area), round(sum_y / area)),
                )
            )

    hotspots.sort(key=lambda item: (item.mean_error * np.sqrt(item.area), item.peak_error), reverse=True)
    return hotspots


def draw_hotspots(base_img: Image.Image, hotspots: list[Hotspot], limit: int = 12) -> Image.Image:
    image = base_img.convert("RGBA")
    draw = ImageDraw.Draw(image)
    font = ImageFont.load_default()

    for idx, hotspot in enumerate(hotspots[:limit], start=1):
        x0, y0, x1, y1 = hotspot.bbox
        pad = 3
        box = (x0 - pad, y0 - pad, x1 + pad, y1 + pad)
        draw.rectangle(box, outline=(255, 240, 120, 255), width=2)
        label = f"{idx} {hotspot.mean_error*100:.1f}%"
        tw = int(draw.textlength(label, font=font))
        label_box = (x0 - pad, max(0, y0 - 18), x0 - pad + tw + 8, max(0, y0 - 18) + 16)
        draw.rectangle(label_box, fill=(18, 20, 28, 220))
        draw.text((label_box[0] + 4, label_box[1] + 2), label, fill=(255, 240, 120, 255), font=font)

    return image


def compose_compare(reference: Image.Image, shot: Image.Image, spectral: Image.Image, signed: Image.Image) -> Image.Image:
    gap = 18
    title_h = 26
    tile_w, tile_h = reference.size
    sheet = Image.new("RGB", (tile_w * 2 + gap * 3, tile_h * 2 + gap * 3 + title_h * 2), (10, 12, 12))
    draw = ImageDraw.Draw(sheet)
    font = ImageFont.load_default()

    items = [
        ("Reference", reference),
        ("Shot", shot),
        ("Spectral X-Ray", spectral),
        ("Signed X-Ray", signed),
    ]

    for idx, (title, image) in enumerate(items):
        row, col = divmod(idx, 2)
        x = gap + col * (tile_w + gap)
        y = gap + row * (tile_h + gap + title_h)
        draw.text((x, y), title, fill=(235, 220, 150), font=font)
        sheet.paste(image.convert("RGB"), (x, y + title_h))

    return sheet


def write_hotspots(path: Path, hotspots: list[Hotspot], image_size: tuple[int, int], threshold: float) -> None:
    w, h = image_size
    lines = [
        "Spectral x-ray hotspots",
        "=======================",
        "",
        f"image size: {w}x{h}",
        f"threshold: {threshold * 100:.1f}%",
        "",
        "top hotspots",
        "------------",
    ]
    for idx, hotspot in enumerate(hotspots[:20], start=1):
        x0, y0, x1, y1 = hotspot.bbox
        cx, cy = hotspot.centroid
        lines.append(
            f"{idx:02}  mean={hotspot.mean_error*100:5.2f}%  peak={hotspot.peak_error*100:5.2f}%  "
            f"area={hotspot.area:6d}  bbox=({x0},{y0})-({x1},{y1})  centroid=({cx},{cy})"
        )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate a full-spectrum x-ray diff between a reference image and a renderer shot.")
    parser.add_argument("--reference", type=Path, required=True)
    parser.add_argument("--shot", type=Path, required=True)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument("--threshold", type=float, default=0.12, help="Hotspot threshold as normalized error (0-1).")
    parser.add_argument("--min-area", type=int, default=180, help="Minimum connected component area.")
    args = parser.parse_args()

    args.out_dir.mkdir(parents=True, exist_ok=True)

    reference = load_rgb(args.reference)
    shot = load_rgb(args.shot)
    if reference.shape != shot.shape:
        raise SystemExit(f"Image sizes must match; got {reference.shape} vs {shot.shape}")

    delta = shot - reference
    error_map = np.mean(np.abs(delta), axis=2) / 255.0
    spectral = hsv_rainbow(error_map)
    signed = signed_xray(delta)
    hotspots = find_hotspots(error_map, threshold=args.threshold, min_area=args.min_area)

    ref_img = Image.fromarray(reference.astype(np.uint8), mode="RGB")
    shot_img = Image.fromarray(shot.astype(np.uint8), mode="RGB")
    spectral_img = Image.fromarray(spectral, mode="RGB")
    signed_img = Image.fromarray(signed, mode="RGB")
    boxed_img = draw_hotspots(spectral_img, hotspots)
    compare_img = compose_compare(ref_img, shot_img, boxed_img.convert("RGB"), signed_img)

    ref_img.save(args.out_dir / "spectral-reference.png")
    shot_img.save(args.out_dir / "spectral-shot.png")
    spectral_img.save(args.out_dir / "spectral-xray.png")
    signed_img.save(args.out_dir / "signed-xray.png")
    boxed_img.save(args.out_dir / "spectral-xray-boxed.png")
    compare_img.save(args.out_dir / "spectral-xray-compare.png")
    write_hotspots(args.out_dir / "spectral-hotspots.txt", hotspots, ref_img.size, args.threshold)


if __name__ == "__main__":
    main()
