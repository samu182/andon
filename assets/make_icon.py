"""構成図の窓のアイコンを作る（四角 3 つを線でつなぐ。白地に紺 1 色）。

  python assets/make_icon.py

assets/icon.ico（exe に埋め込む・16〜256 画素）と assets/icon-64.rgba（窓の題の帯とタスクバー・64×64 の RGBA）を書く。
小さく縮めても線が潰れないよう、4 倍で描いてから縮める。
"""
from pathlib import Path

from PIL import Image, ImageDraw

HERE = Path(__file__).resolve().parent
BONE = (30, 58, 138, 255)  # 紺（窓の中のアクセントと同じ #1e3a8a）
PAPER = (255, 255, 255, 255)
EDGE = (191, 197, 205, 255)  # 部品の枠と同じ灰色（#bfc5cd）。暗いタスクバーでも白地の形が分かるように


def draw(size):
    k = 4  # 4 倍で描いてから縮める
    s = size * k
    u = s / 256  # 256 を基準にした寸法
    im = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    # 白い角丸の地
    d.rounded_rectangle([8 * u, 8 * u, 248 * u, 248 * u], radius=44 * u, fill=PAPER, outline=EDGE, width=max(1, round(6 * u)))
    # 線: 左の四角から分かれて右の 2 つへ
    w = max(1, round(14 * u))
    d.line([(96 * u, 128 * u), (128 * u, 128 * u)], fill=BONE, width=w)
    d.line([(128 * u, 76 * u), (128 * u, 180 * u)], fill=BONE, width=w)
    d.line([(128 * u, 76 * u), (160 * u, 76 * u)], fill=BONE, width=w)
    d.line([(128 * u, 180 * u), (160 * u, 180 * u)], fill=BONE, width=w)
    # 四角（部品）
    r = 12 * u
    d.rounded_rectangle([36 * u, 100 * u, 100 * u, 156 * u], radius=r, fill=BONE)
    d.rounded_rectangle([156 * u, 48 * u, 220 * u, 104 * u], radius=r, fill=BONE)
    d.rounded_rectangle([156 * u, 152 * u, 220 * u, 208 * u], radius=r, fill=BONE)
    return im.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [draw(n) for n in sizes]
    images[-1].save(HERE / "icon.ico", sizes=[(n, n) for n in sizes], append_images=images[:-1])
    (HERE / "icon-64.rgba").write_bytes(draw(64).tobytes())
    draw(256).save(HERE / "icon-preview.png")
    print("wrote", HERE / "icon.ico", HERE / "icon-64.rgba")
