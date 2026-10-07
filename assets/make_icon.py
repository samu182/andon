"""あんどんの窓のアイコンを作る（行灯を 16×16 の低ドットで。白地に紺と灯りの色）。

  python assets/make_icon.py

assets/icon.ico（exe に埋め込む・16〜256 画素）と assets/icon-64.rgba（窓の題の帯とタスクバー・64×64 の RGBA）を書く。
ドットがにじまないよう、16×16 を整数倍にだけ拡大する（ぼかさない）。
"""
from pathlib import Path

from PIL import Image

HERE = Path(__file__).resolve().parent
COLORS = {
    ".": (0, 0, 0, 0),
    "g": (191, 197, 205, 255),  # 地の縁（部品の枠と同じ灰色 #bfc5cd）。暗いタスクバーでも白地の形が分かるように
    "w": (255, 255, 255, 255),  # 地
    "n": (30, 58, 138, 255),  # 枠（窓の中のアクセントと同じ紺 #1e3a8a）
    "y": (253, 210, 100, 255),  # 灯りのついた紙（色はここだけ）
    "c": (255, 240, 190, 255),  # 灯りの芯
}
DOTS = [
    "..gggggggggggg..",
    ".gwwwwwwwwwwwwg.",
    "gwwwwwwnnwwwwwwg",  # 取っ手
    "gwwnnnnnnnnnnwwg",  # 笠
    "gwwwnyyyyyynwwwg",
    "gwwwnyyccyynwwwg",
    "gwwwnyccccynwwwg",
    "gwwwnnnnnnnnwwwg",  # 桟
    "gwwwnyccccynwwwg",
    "gwwwnyyccyynwwwg",
    "gwwwnyyyyyynwwwg",
    "gwwnnnnnnnnnnwwg",  # 台
    "gwwwnwwwwwwnwwwg",  # 脚
    "gwwwnwwwwwwnwwwg",
    ".gwwwwwwwwwwwwg.",
    "..gggggggggggg..",
]


def draw(size):
    assert size % 16 == 0, "16 の整数倍だけ（ドットをにじませない）"
    im = Image.new("RGBA", (16, 16))
    im.putdata([COLORS[ch] for row in DOTS for ch in row])
    return im.resize((size, size), Image.NEAREST)


if __name__ == "__main__":
    sizes = [16, 32, 48, 64, 128, 256]
    images = [draw(n) for n in sizes]
    images[-1].save(HERE / "icon.ico", sizes=[(n, n) for n in sizes], append_images=images[:-1])
    (HERE / "icon-64.rgba").write_bytes(draw(64).tobytes())
    draw(256).save(HERE / "icon-preview.png")
    print("wrote", HERE / "icon.ico", HERE / "icon-64.rgba")
