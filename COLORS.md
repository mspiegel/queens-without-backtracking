# Color → Letter Mapping

Each cell in `archive/boards/<id>.txt` is a single uppercase letter representing one of LinkedIn Queens' colored regions. Same letter across different puzzles means the same color family.

## Primary palette (12 letters)

These letters appear across all three source archives (archivedqueens, zipgame, queensgame). The hex values differ slightly between sites — same color family, different exact shade.

| Letter | Mnemonic        | archivedqueens | zipgame    | queensgame |
|--------|-----------------|----------------|------------|------------|
| P      | Purple          | `#BBA3E2`      | `#D7CCFF`  | `#BBA3E2`  |
| O      | Orange          | `#FFC992`      | `#FFD8B2`  | `#FFC992`  |
| B      | Blue            | `#96BEFF`      | `#C7E1FF`  | `#96BEFF`  |
| G      | Green           | `#B3DFA0`      | `#C9F0C2`  | `#B3DFA0`  |
| S      | Silver (gray)   | `#DFDFDF`      | `#E4E6EB`  | `#DFDFDF`  |
| R      | Red (coral)     | `#FF7B60`      | `#FFB7A1`  | `#FF7B60`  |
| L      | Lime            | `#E6F388`      | `#F6F1A6`  | `#E6F388`  |
| K      | Khaki (tan)     | `#B9B29E`      | `#D7C8B5`  | `#B9B29E`  |
| N      | piNk            | `#DFA0BF`      | —          | `#DFA0BF`  |
| T      | Teal            | `#A3D2D8`      | —          | `#95CBCF`  |
| C      | Cyan            | `#62EFEA`      | —          | `#55EBE2`  |
| Y      | Yellow          | `#FFE046`      | —          | `#DCF079`  |

Zipgame's CSS defines only 8 color classes, so its palette stops at `K`. This is why 9×9 boards from zipgame were unusable (documented in `archive/MISSING.md`).

## Extended palette (9 additional letters — queensgame only)

Variant shades (e.g. a second purple, a second red) that LinkedIn occasionally uses alongside the primary palette.

| Letter | Mnemonic      | Hex       |
|--------|---------------|-----------|
| U      | Ultraviolet   | `#AF96DC` |
| Q      | sQuash        | `#FBBF81` |
| A      | Azure         | `#85B5FC` |
| J      | Jade          | `#A6D995` |
| W      | White-gray    | `#D9D9D9` |
| V      | Vermilion     | `#F96C51` |
| M      | Mauve         | `#D895B2` |
| F      | Fuchsia       | `#FE93F1` |
| H      | Hazel         | `#ADA68E` |

## Design notes

- **Letter choice**: mnemonics favor the color's first letter, with fallbacks when a letter is taken. `S` (not `G`) for gray avoids clashing with Green; `N` (not `P`) for pink avoids Purple; `K` for khaki (its natural initial); `L` for lime (not yellow-green's second word).
- **Collision-free**: no board uses the same letter for two distinct regions. Within a given `<id>.txt` each letter represents exactly one colored region.
- **Cross-archive consistency**: same letter = same color family, even when the underlying hex differs slightly between the three source sites. A letter-by-letter match between two `<id>.txt` files from different sources was the verification method used during consolidation.
