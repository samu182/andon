"""あんどんの窓のアイコンを作る（行灯: 紺の枠に、灯りのついた紙。白地に紺と灯りの色）。

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
GLOW = (253, 224, 138, 255)  # 灯りのついた紙（色はここだけ）
CORE = (255, 243, 205, 255)  # 灯りの芯（紙の真ん中を明るく）


def draw(size):
    k = 4  # 4 倍で描いてから縮める
    s = size * k
    u = s / 256  # 256 を基準にした寸法
    im = Image.new("RGBA", (s, s), (0, 0, 0, 0))
    d = ImageDraw.Draw(im)
    # 白い角丸の地
    d.rounded_rectangle([8 * u, 8 * u, 248 * u, 248 * u], radius=44 * u, fill=PAPER, outline=EDGE, width=max(1, round(6 * u)))
    # 紙（灯りの色。真ん中を明るく）
    d.rectangle([84 * u, 58 * u, 172 * u, 194 * u], fill=GLOW)
    d.ellipse([100 * u, 82 * u, 156 * u, 170 * u], fill=CORE)
    # 枠: 紙のまわりと、真ん中の桟
    w = max(1, round(12 * u))
    d.rectangle([84 * u, 58 * u, 172 * u, 194 * u], outline=BONE, width=w)
    d.line([(84 * u, 126 * u), (172 * u, 126 * u)], fill=BONE, width=max(1, round(7 * u)))
    # 上の笠と取っ手
    d.rounded_rectangle([68 * u, 44 * u, 188 * u, 62 * u], radius=6 * u, fill=BONE)
    d.rounded_rectangle([112 * u, 26 * u, 144 * u, 50 * u], radius=6 * u, outline=BONE, width=max(1, round(8 * u)))
    # 下の台と脚
    d.rounded_rectangle([68 * u, 190 * u, 188 * u, 208 * u], radius=6 * u, fill=BONE)
    d.rectangle([76 * u, 206 * u, 94 * u, 228 * u], fill=BONE)
    d.rectangle([162 * u, 206 * u, 180 * u, 228 * u], fill=BONE)
    return im.resize((size, size), Image.LANCZOS)


if __name__ == "__main__":
    sizes = [16, 24, 32, 48, 64, 128, 256]
    images = [draw(n) for n in sizes]
    images[-1].save(HERE / "icon.ico", sizes=[(n, n) for n in sizes], append_images=images[:-1])
    (HERE / "icon-64.rgba").write_bytes(draw(64).tobytes())
    draw(256).save(HERE / "icon-preview.png")
    print("wrote", HERE / "icon.ico", HERE / "icon-64.rgba")
