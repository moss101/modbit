# Media fixtures (docs/58)

Deterministic target media for the Media Pipeline tests. Every fixture is hashed here; tests verify the hash before use.

| File | Bytes | sha256 | Purpose |
|---|---|---|---|
| `bomb.png` | 69 | `43cfe8ceebce2245acf5720d55b0744e45d12a84587b97b0861179c43e82e6b6` | pixel bomb: 40000×40000 header with tiny data; the pixel budget must stop decoding |
| `label.png` | 1149 | `8bc24c648d789ae250166f32ce9fc4d5d72811e8367035818ab9fa17c7823fca` | PNG with a fact visible only in pixels, a tEXt chunk and hostile EXIF text |
| `malformed.pdf` | 2057 | `f2a9d588db14043452e41d027f44877bd522900e368e43358e5293467e6b5350` | PDF header followed by deterministic random bytes; typed MEDIA_MALFORMED |
| `many-pages.pdf` | 22276 | `171d2ba9ca5938168bae108e0c988fa2f3a77201e05fd61974de7d7f1f1e8296` | 80-page text PDF for the page budget |
| `photo.jpg` | 1080 | `8d68719bc1599c6e42108718f6914ec27ea996869574658411abb9f8714ba303` | JPEG with EXIF (hostile description, camera model) that egress copies must strip |
| `report.pdf` | 926 | `57b8967d3fc74bc18faa3109dcb7ea994c87060320d9bec3e6fae588979661a3` | two-page text PDF; page 2 carries a hostile instruction that must stay untrusted data |
