"""Paired offscreen sky/shading verification; never opens a game window.

Both probes must already be built. The baseline probe is the same driver with
feature-only calls removed, linked to unmodified baseline engine code. Generated
images/logs go only in --out, never in the source installation or Git.
"""
import argparse
import json
import os
from pathlib import Path
import subprocess
from PIL import Image, ImageChops, ImageDraw

CASES = {
    "skylands": ("synthetic:map_slate/:0", "map_skylands/", ["horizon=0,12,0,100,25,100", "floor=0,30,0,100,-20,100"]),
    "slate": ("synthetic:map_slate/:0", "map_slate/", ["horizon=0,12,0,100,25,100", "floor=0,12,0,20,0,20"]),
    "bedroom": ("synthetic:map_bedroom/:0", "map_bedroom/", ["window=160,370,145,300,380,145", "outside=400,245,125,185,350,125", "floor=95,290,98,0,278,130"]),
    "bedroom-dark": ("synthetic:map_bedroom/:0", "map_bedroomdark/", ["window=160,370,145,300,380,145", "outside=400,245,125,185,350,125"]),
    "kitchen": ("synthetic:map_kitchen/:0", "map_kitchen/", ["inside=-378,123,166,-430,115,160", "outside=-680,140,100,-400,220,100"]),
    "dense": ("Golden Gate Bridge", "map_slate/", []),
    "water": ("synthetic:map_slate/:0", "map_slate_sea_revised/", ["surface=0,10,0,60,-10,60"]),
}
OFF = "off:soft=0,ao=0,original_sky=1"
VARIANTS = OFF + ";soft:soft=1,ao=0,original_sky=1;ao:soft=0,ao=1,original_sky=1;both:soft=1,ao=1,original_sky=1;forced:soft=0,ao=0,original_sky=1,enhanced_sky=1;enhanced:soft=1,ao=1,enhanced_sky=1"


def run(probe, content, out, case, variants, modes, samples=1, width=1920, height=1080, frames=16):
    world, map_id, views = CASES[case]
    out.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    env.update(BRI_MAP=map_id, BRI_VARIANTS=variants, BRI_MODES=modes,
               BRI_MSAA=str(samples), BRI_WIDTH=str(width), BRI_HEIGHT=str(height),
               BRI_FRAMES=str(frames), BRI_TIME="1", WGPU_BACKEND="dx12")
    command = [str(probe), str(content), str(out), world, *views]
    with (out / "probe.log").open("w", encoding="utf-8") as log:
        subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)


def montage(paths, out):
    width, height = 640, 360
    canvas = Image.new("RGB", (width * 3, (height + 26) * ((len(paths) + 2) // 3)), "#171b23")
    draw = ImageDraw.Draw(canvas)
    for i, p in enumerate(paths):
        x, y = (i % 3) * width, (i // 3) * (height + 26)
        with Image.open(p) as im:
            canvas.paste(im.convert("RGB").resize((width, height)), (x, y + 26))
        draw.text((x + 8, y + 6), p.stem, fill="white")
    canvas.save(out)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--preview", type=Path, required=True)
    parser.add_argument("--content", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--phase", choices=["parity", "looks", "timing"], required=True)
    parser.add_argument("--case", choices=CASES, action="append")
    args = parser.parse_args()
    cases = args.case or list(CASES)
    results = []
    for case in cases:
        print(f"{args.phase}: {case}", flush=True)
        folder = args.out / args.phase / case
        if args.phase == "parity":
            for samples in [1, 4]:
                baseline, preview = folder / f"main-msaa{samples}", folder / f"preview-msaa{samples}"
                run(args.baseline, args.content, baseline, case, OFF, "classic,unified,dynamic", samples=samples)
                run(args.preview, args.content, preview, case, VARIANTS, "classic,unified,dynamic", samples=samples)
                for original in baseline.glob("*-off.png"):
                    name = original.name
                    with Image.open(original) as a, Image.open(preview / name) as b:
                        equal = ImageChops.difference(a.convert("RGB"), b.convert("RGB")).getbbox() is None
                    results.append(dict(case=case, samples=samples, image=name, equal=equal))
                    if not equal:
                        raise RuntimeError(f"Off differs from main: {case}/{name}, MSAA {samples}")
                    if "-classic-" in name:
                        for variant in ["soft", "ao", "both"]:
                            with Image.open(original) as a, Image.open(preview / name.replace("-off.png", f"-{variant}.png")) as b:
                                equal = a.tobytes() == b.tobytes()
                            results.append(dict(case=case, samples=samples, image=name, classic_variant=variant, equal=equal))
                            if not equal:
                                raise RuntimeError(f"Classic differs: {case}, {variant}, MSAA {samples}")
                for mode in ["classic", "unified", "dynamic"]:
                    for original in preview.glob(f"*-{mode}-off.png"):
                        forced = original.with_name(original.name.replace("-off.png", "-forced.png"))
                        with Image.open(original) as a, Image.open(forced) as b:
                            if a.tobytes() != b.tobytes():
                                raise RuntimeError(f"Original override differs: {case}/{mode}")
        elif args.phase == "looks":
            variants = VARIANTS + ";sunset:soft=1,ao=1,enhanced_sky=1,sun=0.02;twilight:soft=1,ao=1,enhanced_sky=1,sun=-0.12"
            run(args.preview, args.content, folder, case, variants, "unified,dynamic", frames=24)
            for mode in ["unified", "dynamic"]:
                paths = sorted(folder.glob(f"*-{mode}-*.png"))
                montage(paths, folder / f"{case}-{mode}-montage.png")
        else:
            for width, height in [(1920, 1080), (2560, 1440)]:
                for samples in [1, 4]:
                    target = folder / f"{width}x{height}-msaa{samples}"
                    run(args.preview, args.content, target, case, OFF + ";ao:soft=0,ao=1,original_sky=1;both:soft=1,ao=1,original_sky=1", "unified,dynamic", samples, width, height, frames=160)
    args.out.mkdir(parents=True, exist_ok=True)
    (args.out / f"{args.phase}-results.json").write_text(json.dumps(results, indent=2))
    print(f"{args.phase} completed", flush=True)


if __name__ == "__main__":
    main()
