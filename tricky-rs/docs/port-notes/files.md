# Files: the disc, BIG archives, compression

## The disc (row E2)

Proven (`lib-file`, host code on the PS2; ported 1:1 anyway in `tricky-data/src/disc.rs`):

- `CdFs_Mount` 0x2c9970 runs once (from 0x2cc2c0, at most 1000 files, 0x100 bytes of path per entry, entries
  0x10c bytes). It reads the primary volume descriptor (sector 16), takes the root directory record at +0x9c
  (extent +2, size +10) and walks the tree with `CdFs_ScanDir` 0x2c9200 from the root (the folder prefix at
  0x3bb938 is empty).
- `CdFs_ScanDir`: skips the `.` and `..` records, a zero length byte ends a sector's records. Directories
  (flags +0x19 bit 2) are walked at once with `NAME/` added to the prefix; a file's name loses its last two
  characters (`;1`) and then a trailing `.`. Entry: `CdFs_HashPath(path)`, extent, size, path
  (`/DATA/CHAR/MDLPS2.BIG`).
- `CdFs_HashPath` 0x2c9190: ELF-style hash (`h = h*16 + c; g = h & 0xf0000000; if g: h ^= g>>24 ^ g`) taken
  from the terminating zero back to the second character: the leading `/` is not hashed.
- The table is sorted with the C library's `qsort` 0x2fb5c8 (Bentley-McIlroy) by the 32-bit wrapping hash
  difference (0x2c9460). An equal pair switches to path order: sorted again with `strcmp` (0x2c9420) and
  searched with `strcmp` (0x2c9440). The shipped disc has no collision (260 files).
- `CdFs_FindFile` 0x2c9cc0: drops one leading `/`, turns `\` into `/`, upper-cases a-z, puts `/` in front,
  hashes it and `bsearch`es 0x2fa258 the table (comparison 0x2c9488: key hash minus entry hash). Returns the
  1-based entry number.
- File opens go through EA's async file system (0x2c9018 → 0x2ca7f8 → 0x2cb328): BIG archives mounted with
  `BigArchive_Open` are searched first (`BIG_Lookup` 0x2cbdc0), then the disc (0x2cc4b8). `host0:` paths
  (0x2cc368) are the dev kit's and never used on a retail disc.

Checked: the table of `SSX Tricky (USA).iso` equals an independent ISO 9660 walk (260 files, extents and
sizes); `data/char/mdlps2.big` read through it is byte-identical to the copy extracted before.

Checked in the runner (row E2k, `game/tests/disc_table.rs`): 200 made-up ISO 9660 images (random folder trees
three deep, names with no extension, with a bare `.` and with one, records that end exactly at a sector's end,
folders of several sectors, `AB`/`QA` pairs whose paths hash the same, one disc of 1,200+ files) mounted by
`CdFs_Mount` with `CdFs_ReadSector` 0x2c90c8 answered from the image and the drive calls stubbed. The table
(hash, extent, size, path, in order), the count and the comparison left in 0x349d6c equal `Disc::open`'s; then
`CdFs_FindFile` and `Disc::find` agree on every file and folder in upper and lower case, with `\`, with and
without a leading `/`, with a letter added, and on random, empty, `/` and `//AB` paths (39,379 files found, 158
discs switched to path order). Too many files: the game's `CdFs_Mount` returns 0 files, ours
returns `TooManyFiles`. `qsort` 0x2fb5c8 is also run on its own 3,000 times (0 to 400 elements, many equal ones)
with the wrapping difference, which is no true order, and leaves the same order as ours.

- Proven: 0x2c9460 and 0x2c9488 were not functions in Ghidra (only stored as pointers); made into
  `CdFs_CompareEntryHash` (entry hash minus entry hash, an equal pair stores `CdFs_CompareName` in 0x349d6c) and
  `CdFs_CompareKeyHash` (the searched key's hash minus the entry's) and added by hand to `function-index.csv`
  and `function-systems.csv`. `ApplySsxSymbols.java` only renames existing functions for `G` lines: create them
  by hand (P on the address) in an older project.
- `CdFs_ScanDir` has no end-of-sector test: after a record that ends exactly at the sector's end it reads the
  byte after the sector buffer (0x34a5c0, the CD callback's stack, set up only after the mount) and goes on to
  the next sector when it is 0. Ours ends the sector there (inferred: that byte is 0 during the mount; the runner
  had it 0 and agreed on the two discs with such sectors).
- 35 hand mutants of `disc.rs` (limits, record parsing, the hash, the comparisons, `bsearch`, every branch of
  `qsort`): 34 killed. Left alive: `strcmp` taking bytes as signed, equal on ISO 9660 names (A-Z, 0-9, `_`;
  the game's `strcmp` 0x2fd008 takes them as unsigned, as ours does, and our paths are Rust strings).

## BIG archives (row E2a)

Proven (`lib-big` and the async file system; ported in `tricky-data/src/big.rs`):

- `BIG_Identify` 0x2cbc88: big-endian u16 0xc0fb → `C0FB` (1), `BIGF` → 2. `BIG_HeaderSize` 0x2cbd08: `C0FB` u16
  at +2 plus 4, `BIGF` u32 at +0xc (both big-endian).
- `BIG_Lookup` 0x2cbdc0: `C0FB` directory from +6 (u24 offset, u24 size, name), `BIGF` from +0x10 (u32, u32, name);
  names end with a zero; matched with `stricmp` 0x2fcc98, or by entry number when no name is given. Offsets
  are from the archive's start. `BIG_GetFileData` 0x2cc0a8 takes offset 0 as "not found".
- `App_OpenBigFiles` 0x17bdf8 mounts, in order: `data/audio/speech.big`, `data/audio/audio.big` (if there),
  `data/char/mdlps2.big`, `brdps2.big`, `anm.big`, `texps2.big`, `data/audio/music.big` (if there).
  `FileSys_MountBig` 0x2c8f50 reads the file's start; `FileSys_OnBigHeaderRead` 0x2caca8 reads the whole
  directory again if it is longer, and puts the archive at the head of the mount list (newest searched first).
  Only the directory stays in memory.
- `FileSys_RunQueue` 0x2cb328 opens a path: no `|` → the disc first (`FileSys_OpenOnDisc` 0x2cc4b8 →
  `CdFs_FindFile`), then, only when the open mode has bit 0, every archive; `|name` → every archive, not the
  disc; `archive|name` → the archive mounted under exactly that path (`strcmp`).
- `FileSys_MakeDevicePath` 0x2cc368 strips `cd:` and handles `host0:` (dev kit only) and prepends the current
  folder (0x34eef8) to a path that does not start with `\`, `/` or `~`; on a retail disc the current folder is
  the root. Inferred: our `Files::open` does not keep a current folder.

Checked: all 793 entries of the four character archives read through `Files` equal the archives extracted
before (sizes and byte sums); the three audio archives mount (1146 entries in all).

## Compression (row E2b)

Proven (ported in `tricky-data/src/compress.rs`):

- A packed file starts with a type byte and 0xfb. `Compress_Decode` 0x2c2de0 dispatches on the type with its
  low bit cleared: RefPack 0x10/0x90 (`RefPack_Decode` 0x2c31b0), Huffman 0x30/0x32/0x34/0xb0/0xb2/0xb4
  (`Huff_Decode` 0x2c3730), BTree 0x46/0xc6 (`BTree_Decode` 0x2c3540), 0x2c2d18 for 0x18/0x1a/0x1c/0x98/0x9a/0x9c,
  and a registered codec (gp-0x7fc8) for 0x1e/0x9e.
- `Compress_UnpackedSize` 0x2c2fb0: a 3-byte size (4 bytes with bit 7) at +2, whatever the low bit says.
  `RefPack_Decode` instead skips one more size field when the low bit is set (0x11fb: size at +5) and returns
  that field.
- A second, older Huffman set sits at 0x240ef8–0x242200 (`Huff_DecodeBlock`, `Huff_BuildTree`, ...).
  `Huff_IsCompressed` 0x241d28 returns 100 for the headers 0x30fb–0x35fb and 0 for anything else;
  `Huff_GetUnpackedSize` 0x241dd8 reads the 3-byte big-endian size at +2 (+5 when header bit 0x100 is set). Nothing
  calls either directly (inferred: they sit in a codec table).
  - The replays use this set (`Decompress_Block` 0x241e28 -> `Huff_DecodeBlock` 0x240ef8; row G12b). This is
    the newer `Huff_Decode`'s code without the 4-byte size, so a type with bit 7 reads its size from the bit
    stream like the others. Bit 7 only stops 0xb2 and 0xb4 from summing. The port is
    `compress::huff_decode_block`, with `bytes_read_be`, `huff_block_is_compressed` and
    `huff_block_unpacked_size` for the three header helpers.
  - Checked in the runner (`tests/compress.rs`): the game's decoder and ours give the same bytes on the Huffman
    test's streams, 1,500 of them plus the long runs, with half of each type's streams marked with bit 7. The
    helpers were checked on 20,000 random headers.
  - The encoder half (row G12c; `tricky_data::huffenc`, checked: `tricky-game/tests/huffenc.rs`):
    - `Compress_Block` 0x243a60(src, size, dst, options) is what the replay save calls. The options' first word
      is the delta mode: 1 delta-codes a copy once (`Delta_Encode` 0x241e48: each byte less the one before), 2
      does it twice. The type it writes, 0x32fb or 0x34fb, makes the decoder sum the bytes back. It then calls
      `Huff_Encode` on a new 0x31ec-byte context from `Compress_Alloc`, which is not cleared.
    - `Huff_Encode` 0x2438c0 writes the header: the type, then the size in 24 bits. When the unpacked size
      differs, the type is 0x31fb / 0x33fb / 0x35fb and both sizes are written. A mode outside 0..2 writes type
      0. Then come `Huff_Analyze` (flags 0x39, codes up to 15 bits) and `Huff_WriteTable`.
    - `Huff_Analyze` 0x2426f8, the first pass, counts each byte, each delta from the one before, and each repeat
      run (a run of a byte equal to the last, up to 30,000 more).
    - It then picks the clue, the escape: the start of the longest run of unused byte values. The values after
      it stand for repeat runs of 1, 2, ... Flag 0x20 makes the rarest byte the clue when every value is used.
      Flag 2 clear keeps one clue value, and flag 1 then none. Flag 4 clear would also give the second-longest
      run of free values to near deltas (a byte as a small step from the last).
    - It builds the tree, then drops each run symbol that saves less than half its run in bits
      (`Huff_RunCost` 0x242210: the cheapest split of a run over the run symbols, or the clue and a number).
    - The second pass counts again the way the table will be written. Each repeat run goes to the cheapest of:
      the byte repeated, the clue and a number, or the run symbols. The tree is built again
      (`Huff_BuildTree` 0x241ff8: the two lightest joined each time, depths by `Huff_AssignDepths` 0x241f78).
    - Codes over 15 bits are cut down. The two last found at the deepest level go to one past the deepest code
      still under 15, which moves down with them, and the other deepest goes up one level. Canonical codes
      follow, by length then value.
    - `Huff_WriteTable` 0x243368 writes the clue in 8 bits, then the code count for each length as a number
      (`Huff_WriteVarNum` 0x242320: a 1 after b zeros, then the value less its base in b + 2 bits). For each code
      it writes how many free values to step past. Then the bytes: runs, near deltas, plain codes, and the clue's
      own value escaped (`Huff_WriteClueSymbol` 0x2424d0: the clue, 0, then a 0 bit and the byte). It ends with
      the clue, 0, a 1 bit and padding.
    - `Huff_WriteBits` 0x241e88 collects bits from bit 23 of a word downwards.
    - Nothing it reads is left over from the uncleared context except one word: the second clue (+0x2dd0) takes
      the old clue word, and only the near deltas use it. `Encoder::new` takes the starting bytes.
    - An empty block has one code, the clue, one bit long. The decoder then keeps reading code lengths past the
      end, in the game as in our decoder.
  - Checked in the runner:
    - `Huff_Encode`: 1,200 blocks and the long runs, on contexts full of leftovers. The stream, the bytes
      written and all 0x31ec context bytes match, and the streams decode back.
    - `Compress_Block`: 400 blocks in every delta mode.
    - `Huff_Analyze`: 1,500 cases with other flags and code limits, every context byte compared. The 12 the
      port declines are flag sets the game never passes, where a near symbol reads the function's own locals.
    - `Huff_BuildTree` and `Huff_RunCost`: 3,000 random count tables.
    - This found a runner bug: `mult`/`multu`/`div`/`divu` cleared LO1/HI1. The analysis keeps a cost in LO1
      across a `mult`. The runner now keeps them and has learnt `madd1` / `maddu1`.
- `File_LoadAndUnpack` 0x2c8938 (via `File_Load` 0x2c8a70): opens with mode 1 (so the archives are searched),
  reads the file, and if `Compress_UnpackedSize` is not 0 decodes it into a buffer of that size.
- Every packed file on the disc is RefPack 0x10 (the four character archives, the course archives, `PS2LOAD`,
  `SSXFE`, `TRICK`); nothing uses the other codecs (row E2c).
- The other three (row E2c; `compress::{huff_decode, btree_decode, decode18}`, checked: `tricky-game/tests/
  compress.rs`, streams made by the test unpacked by the game's `Compress_Decode` and ours, byte for byte):
  - Huffman (`Huff_Decode` 0x2c3730): big-endian 16-bit reads, most significant bit first. Size: 4 bytes at +2
    with bit 7 (+6 with bit 0), else the bit stream's first 24 bits (from +2, +5 with bit 0). Then the escape
    byte; per code length from 1 a number − 4 codes, until the tree is full; per code a number − 3 not-yet-used
    byte values to step on from the last (from 0xff, wrapping). A number: `1ab` → 4 + ab, else z zeros, a one and
    n = z + 2 bits → bits + 2^n; with 16 or more zeros the game counts them one by one and gets n = z + 3, so no
    number from 2^18 to 2^19 − 1 can be written. Codes ≤ 8 bits come from a 256-entry table, longer ones by
    comparing the top 16 bits with each length's last code. After the escape code a number − 4: not 0 repeats the
    last byte that often, 0 then a bit: 0 an 8-bit literal, 1 the end. 0x32fb sums the bytes once, 0x34fb twice;
    the type is compared with bit 7 kept, so 0xb2/0xb4 never do.
  - BTree (`BTree_Decode` 0x2c3540, `BTree_EmitSymbol` 0x2c34d8): a 3-byte size at +2 (+5 for 0x47fb only; bit
    7 does not make it 4 bytes), the escape byte, a count and (byte, left, right) for each byte that stands for a
    pair; then bytes: plain, pairs (expanded recursively), or the escape and a literal (0 ends).
  - 0x2c2d18 (`Compress_Decode18`): 16-bit pictures in DXT1-like 4×4 blocks: a 3-byte size at +2, the width at
    +6, 8-byte blocks from +8 (two RGB565 colours, four rows of 2-bit picks), rows of blocks until the size is
    reached. The middle colours are 5:3 and 3:5 mixes when colour 0 > colour 1, else the average and 0 (clear);
    a 0 in colour 0 or 1 becomes 1 first; out as PS2 1-5-5-5 (`0x8000 | b << 10 | g << 5 | r`, 0 stays 0).
    `Compress_IsType18` returns 100 (0x1afb) or 0 but `Compress_Decode18Rows` checks for 3 or 5 (blocks with 8
    more bytes before them), so that form is never used.

Checked: all 793 entries of the character archives unpack to the same bytes as `tools/afl/refpack.py`.

Checked in the runner (row E2q, `tricky-game/tests/refpack.rs`):

- `RefPack_Decode` through `Compress_DecodeApi` 0x2c2f90 → `Compress_Decode`: 4,000 streams written by the test
  (0x10fb, 0x11fb, 0x90fb, 0x91fb; every command with 0–3 literals, the longest copies, offsets up to 0x20000,
  copies overlapping what they write, every literal run, every end; 329 streams past 128 KiB, 176 MB in all).
  Same bytes, nothing written past the end, same answer (the size field).
- `Compress_UnpackedSizeApi` 0x2c3190 → `Compress_UnpackedSize`: on those streams and on 20,000 headers of every
  type byte, with and without 0xfb. Proven there: 0x1c and 0x9c, which `Compress_Decode` does decode, have no size
  (0), so the loader takes such a file as not packed; 0x1e / 0x9e have one only when the registered codec is set
  (never).
- `Compress_Decode` on every type it does not take (and every mark other than 0xfb): 0, nothing written.
- The two wrappers are plain calls (`jal`, nothing else).
- 19 hand mutants of `refpack_decode`, `unpacked_size` and `decode`'s dispatch: all killed.

## Character and board models, `.mpf` (rows E4a, E4d)

Proven (loader, renderers, and VU1 chain `0x318080`; see also `notes/characters.md`, verified by
`tricky-game/tests/vu1mpf.rs` on `SLUS_203.26` + `TRICKY_DISC`):
- File header (+4 part count, +6 directory, +8 data), 0x50-byte part entries (`+0x48` morph target count: 23 on
  all face parts, 0 elsewhere), `MpfFile_AddPart`'s name match (`strnicmp` 0x2fd398 on 15 characters), and the
  part tables `SkinPart_Load` 0x205080 fixes up:
  - texture sets at the data's start, 0x20 each (count +0x44): five 4-character names, gloss strength at +0x14;
  - bones at +0x18, 0x54 each (count +0x42): name, +0x10 parent slot, +0x12 parent index, +0x18 position, +0x24
    rotation (Euler, inverse); a bone is named `slot << 8 | index`, the slot being a part's +0x4a (body 0,
    eyes 5, goggles 6, hair 7 or 8);
  - weight sets at +0x30, 0xc each (count +0x3c): (count, pointer) of u32 `bone << 16 | percent`;
  - weight palettes at +0x2c, 8 each (count +0x3e): (count, pointer) of weight-set numbers, one per packet list;
  - materials at +0x20, 0x14 each (count +0x40): +0 flags (`0x1` regular, `0x100` face morph, `0x11` shadow),
    +4 texture set, +0xc / +0x10 its packet lists: (pointer, count) of 0xc-byte records where `+0` is the base
    DMA chain offset, `+4` is the first morph chain's offset from `+0` (fixed up in `SkinPart_Load` as
    `part_data + rec[0] + rec[1]`), and `+8` is the byte stride between morph targets (`0xffffffff` on non-face
    records).
- The names of the parts a rider is built from (`BdrLoad_BuildSkin` 0x201138): tables 0x341318 (body, three
  per level of detail), 0x341588 (head, five) and 0x341998 (board), copied into `mpf::names`.
- **VU1 chain `0x318080`** (programs 8 and 9, `BASE 0xbe`, `OFFSET 0x16a`, 941 instruction pairs, uploaded by
  `PS2Draw_WriteTexState` 0x1c5c88):
  - `0x00c` -> `0x00e`: matrix palette pre-transform (`cPS2GraphicsMan::vf100_1df2d8` 0x1df2d8 / `vf99_1dee80`
    0x1dee80 unpack `n` 4×4 skin matrices at `0x0e` = `PALETTE_BASE = 14` and `n` at `0x3ff`): multiplies each
    by MVP (`VU1[0..3]`) in place at `0x0e + slot*4`, light-direction matrix (`VU1[7..9]`) into `0x72 + slot*3`,
    and envmap matrix (`VU1[0x393..0x395]`) into `0x396 + slot*3`.
  - `0x000` -> `0x096` (multi-bone, 1,373 batches) and `0x00a` -> `0x163` (single-bone, 289 batches): regular
    skinned mesh draw (`cPS2GraphicsMan::vf91_1df478` 0x1df478). Four unpacks per batch: `V4-32` header (GS
    GIFTAG `NREG=3 ST/RGBAQ/XYZ2 TRISTRIP`, `[n_strips, single_mat, single_nrm, n_verts]`, strip lengths),
    and interleaved at `STCYCL 3/1`: `V4-16` UV (`itof12` `/4096`, `z` = `0x0e + slot*4`, `w` = `0x72 + slot*3`),
    `V3-16` normal (`itof15` `/32768`), `V3-32` position (cm). Sets `ADC = 0xffff` on vertices 0 and 1 of each
    strip, and `fcand 0x3ffff` from vertex 2 onward. Followed by `MSCAL 0x6` (`0x251`, gloss/envmap pass) and
    optional `MSCAL 0x8` (`0x160`, `XGKICK 0x392`).
  - `0x004` -> `0x278`: **shadow mesh** draw (`flags & 0x10`, program 9, `cPS2GraphicsMan::vf93_1dfe30` 0x1dfe30 +
    `vf94_1dff88` 0x1dff88, 109 batches / 14,768 verts across all 39 unique shadow materials). Two `V4-32`
    unpacks and `MSCAL 0x4`: header (GS GIFTAG `NREG=1 XYZ2 TRISTRIP`, strip count, strip lengths) and one
    `[pos.x, pos.y, pos.z, mat_addr]` quadword per vertex (`slot = (mat_addr - 14) / 4`), decoded into
    `MpfMesh::shadow_lists`.
  - `0x002` -> `0x06b`: **face morph** expression blender (`flags & 0x100`, `cPS2GraphicsMan::vf91_1df478` 0x1df478,
    252 records × 23 targets = 5,796 morph chains across all 16 face materials). Base record chains end in `ret`
    (no `MSCAL`); each target chain at `rec[0] + rec[1] + t * rec[2]` unpacks `V3-8` (`0x6a`, signed 8-bit) at
    `0x80b6` and runs `MSCAL 0x2`. `0x1df478` uploads `(weight, part_scale * 2560.0)` to `VU1[0x3ff]` (`w[0]`
    unconditionally, `w[1..22]` when `> 0.01`) and VU1 adds `itof15(delta_i8) * weight * part_scale * 2560.0`
    (`0.078125` cm per unit at weight 1, scale 1) directly onto the unpacked base positions before `MSCAL 0x96`,
    decoded into `MpfMesh::morphs`.

Checked:
- For all 12 exported riders and 6 board shapes, `CharModel::rider_from_disc` / `board_from_disc` equal the
  exports (`character::tests::disc_models_match_the_exports`).
- Every `.mpf` file on the disc (`MDLPS2.BIG`, `BRDPS2.BIG`) decodes cleanly including all shadow meshes and
  5,796 face morph targets (`mpf::tests::every_model_decodes`, `vu1mpf::audit_all_mpf_chains_on_disc`).
- Real regular (multi-bone `0x000` and single-bone `0x00a`), shadow (`0x004`), and morphed face (`0x002` +
  `0x096`) chains run through `SLUS_203.26`'s VU1 chain `0x318080` match `tricky_data::mpf` vertex-for-vertex
  (`tricky-game/tests/vu1mpf.rs`).

## Textures, `.ssh` (row E3b)

Proven (`lib-shape`, `tricky-data/src/ssh.rs`): `SHPS`, total size, count, version, then (name[4], offset)
pairs (`Shape_GetEntry` 0x2c266c). A shape is a chain of blocks; each block's first word holds the type (low
byte) and the offset to the next block (upper 24 bits, 0 = last). `Shape_GetLongName` 0x2c2610 finds type 0x70
(name at +4); `Shape_FindByName` 0x2c2528 compares the long name if there is one, else the 4-character name
(`strcmp`). `Shape_Unpack` 0x2c2318 unpacks blocks with bit 0x80 (codecs) and converts types 0x60-0x62.

Every `.ssh` on the disc and in its archives (807 files) uses only: 0x02 (8-bit indices), 0x21 (its 256-colour
palette, RGBA bytes in order), 0x05 (RGBA), 0x70 (long name), 0x7c (hotspots). Colours and alpha are stored as
the GS takes them, 0-0x80 (`TexMan_UploadTexture` 0x1c3ba8 uploads PSMT8 / PSMCT32 as they are).

The textures exported before (`chars/*/*.png`) are these images with colour `c * 255 / 128` rounded down and
alpha doubled (capped at 255): checked equal for all 192 rider textures (12 riders: suit and boots of outfit 1,
head, helm, 12 boards). That mapping is a stand-in for the GS's texture function (`Image::rgba_full_range`).

Also found: a `BIGF` directory's size (+0xc) can end inside the last entry's name; the game reads on into the
rest of its first read, so `Files::mount` keeps the whole first read as the header buffer.

### The shape library on the game's bytes (row E2r; `tricky_game::shapelib`, checked: `tests/shapelib.rs`)

Proven:

- `Shape_GetCount` 0x2c2660 (+8), `Shape_GetEntry` 0x2c266c (file + the entry's offset; 0 for an index at or past
  the count, compared unsigned), `Shape_GetEntryName` 0x2c2694 (the 4 characters stored as a word, no 0 after them;
  0 past the count), `Shape_GetLongName` 0x2c2610 (from the block given along the chain to the first type-'p'
  block, its +4; the next offset is the word's top 24 bits, signed), `Shape_FindByName` 0x2c2528 (the first shape
  whose long name, or 4-character name cut at a 0 when it has none, `strcmp`s equal; `Shape_Find` 0x2c25f0 is a
  plain call of it). A shape with a long name is not found by its 4-character name.
- `Shape_PixelFormatIndex` 0x2c26d8: 4 → 1, 8 → 2, 15 / 16 / 555 / 565 / 1555 → 3, 24 / 888 → 4, 32 / 8888 → 5,
  anything else 0.
- `Shape_Unpack` 0x2c2318, packed half (type bit 0x80): a 0x18fb / 0x28fb stream with mip levels (+0xc top 4 bits)
  goes to `Shape_UnpackBlocks`; any other: `Compress_UnpackedSize` + 0x10 bytes allocated under the name at
  0x3bb918 (flags: the word at 0x3bb978), the header copied, the stream unpacked after it, a 0x1afb stream's type
  byte set to 3; then the type byte loses 0x80 and, when the source block had a next offset, the whole first word
  becomes just the type. A packed 0x1cfb / 0x9cfb stream has no size (0x10 bytes allocated) but `Compress_Decode`
  still unpacks all of it, over whatever follows in the heap.
- `Shape_UnpackBlocks` 0x2c2088: (size + 0x10) × 2 bytes. Level 0's stream is its 8-byte header and a quarter of
  its size (colour blocks pack 4 to 1). Level k's header is the first's written over the last 8 bytes of the level
  before, with the width / 2^k and the size / 4^k (both big-endian), its pixels unpacked after the levels before;
  only the first 4 of the 8 bytes are put back, so the source keeps the header's size low byte, byte 5 and width
  in place of 4 data bytes. A level whose width reaches 0 never ends (`Compress_Decode18Rows` adds 0 per row), in
  the game as in the port. 0x28fb is not a type `Compress_Decode` takes: nothing is unpacked, sizes 0.
- `Compress_Decode18Rows` writes every byte from the first to the last block's end: a row of blocks overruns a
  width that is not a multiple of 4 into the next pixel rows, and the next row of blocks starts where that ends,
  so no byte in between is left as it was.
- Checked in the runner against our heap (`eamem`): 300 shape files (5,619 names found, cut names, repeated
  names, indexes past the count), 1,500 first blocks of six kinds (plain, 0x60..0x62 at every size up to 40 × 40,
  RefPack 0x10 / 0x90, colour-block 0x1a / 0x18 / 0x1c streams, 0x18fb and 0x28fb with 1..4 levels), every pixel
  format to 0x3000; memory and answers equal after each call. 19 mutants: 18 killed, 1 equivalent (signed or
  unsigned quarter of a size under 2^24).

## Text, `.loc` (row E5a)

Proven (`tricky-data/src/loc.rs`): `Loc_LoadForRegion` 0x17c150 loads `data/lang/american.loc` (region 0) or
`british.loc` (region 1, not on the NTSC disc) whole. +8 flags (bit 0: +4 is an id map, `Loc_MapId` 0x2c50b0,
a `bsearch` over (u16 key, u16 value) pairs); +0xe u16 picks the string block (offset at +0x10 + 4 * it); a
block has +0xc the count and from +0x10 the strings' offsets; strings are 16-bit characters ending in 0.

`american.loc` has 3,774 strings. The disc also has two files the exe never names:
- `CONSTANT.LOC`: the same table with the developers' name of every string id (`kHUDKnockdown` 0xe93,
  `kOvCombo` 0xe41, `kOVInvalidCard` 0x16e, ...). Use it to read which text a `Loc_GetString` call asks for.
- `LETTER.LOC`: the English text with a few strings changed (a variant, unused).

### EA's text library (row E5f; `tricky_game::wstr`, checked: `tests/wstr.rs`)

Proven (16-bit strings, `lib-text`):

- `WStr_Len` 0x2c52f8, `WStr_NCmp` 0x2c5438 (the difference of the first differing characters within n, stopping
  at the first string's end; n ≤ 0 gives 0), `WStr_Cmp` 0x2c5418 (n = 0x7fffffff), `WStr_Find` 0x2c5380 (the
  first index, −1; always −1 in an empty string, 0 for an empty needle otherwise), `WStr_NCopy` 0x2c54b8 /
  `WStr_CopyAll` 0x2c5498 (a 0 always written), `WStr_FromAscii` 0x2c5510, `WStr_FormatArgs` 0x2c5578 (`%1` and
  `%2` replaced by two strings, any other `%` kept; `WStr_FormatThunk` 0x2c5558 passes 0 for the second).
- The wide `printf`, `WStr_VFormat` 0x2c48e8(dst, fmt, args in 8-byte slots) and `WStr_Sprintf` 0x2c4e90 (spills
  a2..t3 as the slots). Flags `-` 0x10, `+` 1, ` ` 4, `#` 2, `0` 8; width and precision from `*` (an int slot) or
  digits that start with 1-9. A precision written `.0…` is none: the 0 becomes the conversion and is printed as
  it is (`%.0f` prints `0f`, taking no argument). Unknown conversions print themselves (`%%` → `%`); a `%` at the
  end prints a 0 character and reads on past the format's end.
- Every number goes through a double: `__floatsidf`, then `__fixdfsi` (signed) or `__fixunsdfsi` (unsigned, 0 for
  anything negative): so `%u` `%x` `%o` `%p` print 0 for 0x80000000 and up. Only `%f` (flag 0x40) prints a
  fraction: `%e` `%E` `%g` `%G` print the integer part like `%d` (−0.5 prints `-0`). The precision only reaches
  `%f` (default 6). `%p` / `%P` are 8 wide, zero-padded, whatever the width. `%c` takes 16 bits, `%C` 8, `%s` a
  16-bit string, `%S` an 8-bit one, `(null)` for 0; `%n` stores the count so far.
- `Fmt_WriteNumber` 0x2c45e0 builds the number backwards in a 127-character stack buffer: with `0` (and not `-`)
  zeros up to the width first, so the sign comes on top (`%05d` of −42 is `-00042`); the sign (`-` below 0; with
  `+`: `+` above 0 and a space at 0; with ` ` alone a space at 0 and above), `0` / `0x` with `#` in base 8 / 16;
  then padding to the width (zeros with `0`, else spaces), or spaces after the copy with `-`. Lengths are
  `WStr_Len` of the buffer, so a 0 character in it ends the number.
- `Fmt_WriteFixed` 0x2c4508(p, x, prec) writes decpt + prec + 1 characters backwards from `fcvt`'s digits
  ('0' past their end), the point at prec. So there is no leading 0 (0.5 → `.500000`); below 0.1 the point is
  never written and the first character is the byte before `fcvt`'s buffer (inferred: the top byte of the newlib
  heap chunk's size word, 0), which hides the whole number (0.05 prints nothing, or the width's padding). When
  decpt + prec < −1 (0.005 at 0 places, under 1e-7 at 6) the loop never ends; the port panics there.
- `fcvt` 0x2fa330 → `fcvtbuf` 0x303658 (not newlib's usual): between −1 and 1 exclusive `_dtoa_r` mode 2 with
  prec *significant* digits, else mode 3 with prec digits after the point; trailing zeros dropped; then zeros
  added until the string is prec long. Rounding to nearest, ties to even (Rust's exact formatting gives the same
  digits on every case tried). Its buffer grows with `_realloc_r` when prec ≥ its length.
- Checked in the runner: 20,000 helper calls and 100,000 random formats (every flag, `*` widths −6..39 and
  precisions 0..7, every conversion and 3 unknown ones, a `%` at the end, null strings, `%n`; a third through
  `WStr_Sprintf`), 2.57 million characters written; the C library's heap answered by a bump allocator with
  newlib-like chunk headers. 37 mutants: 31 killed, 6 equivalent (`WStr_NCmp` stopping at either string's end,
  `fcvt`'s zero stripping and padding beyond the digits read, the `n != −1` guard past the panic, the second
  padding's zeros after the first filled the width, a width starting with 0 that the flag takes first).

## Trick book, `TRICKDEF.DAT` (row E5d)

Proven (`tricky-data/src/trickdef.rs`): `TrickDef_Load` 0x154580 reads `data/tutorial/trickdef.dat` whole
(0x2760 bytes: 12 riders × 30 tricks × 28 bytes). `Score_FormatTrickName` 0x1551c0 prints fourteen `%s` from
pointer tables at 0x320970-0x320f58 (read from the disassembly: rail-to `w1>>12&15` at 0x320b08, rail spin
`w1>>4&15` 0x320b20, stance `w1>>16&15` 0x320970, spin side `w1>>27&7` 0x320988, spin `w0>>24&15` 0x320998,
flip count `w1>>24&7` 0x3209c8, flip kind `w1>>8&15` 0x3209d8, combo `w0&255` 0x3209f0, spin after `w0>>28`
0x320aa8, grab `w0>>8&255` 0x320b58, late `w1>>30` 0x320b50, second grab `w0>>16&255`, ending `w1>>20&15`
0x320ad8, crash `w1&15` 0x320ae8). The tables are adjacent and an index past one reads the next, so
`trick_names.rs` keeps the whole region. All 360 names equal `chars/trickbook.json`.

## Fonts, `.SFN` (row E5e)

Proven (`tricky-data/src/sfn.rs`): `cGame::Init` 0x17e148 loads `data/fonts/title.sfn` (gApp+0x5c, scale
1.4 x 1.3) and `data/fonts/menu.sfn` (gApp+0x60, 1.8 x 1.4). `Font_ParseGlyphs` 0x19c910: +8 version (records
of 12 bytes from 200, 11 before), +0xa count, +0x10/+0x11 origin, +0x14 records, +0x1c bitmap.
`Font_BuildTexture` 0x19cca8: 4-bit pixels, low nibble first, into a square texture (side: max of the width and
the smallest power of two above the height), colours RGB 0x80 with alpha `i * 128 / 15`. The glyphs and line
heights equal `chars/fonts/*.json` for both fonts.

## Camera files, `.CML` (row E5c)

Proven: `Camera_LoadCmlFiles` 0x1783e8 loads, in order, the course's file (`Camera_GetCourseCmlName` 0x178330:
course id 0 garibald, 1 snowdrea, 2 elysium, 3 mesablan, 4 merqury, 5 aloha, 6 and 9 pipedrea, 7 untracke, 8
tokyo, 10 trick, 11 alaska), `commonob` and `scripts`, each `%s.cml` (0x37fba8) in DATA/CAMERA, skipping a
missing one (`Cml_LoadIfExists` 0x16e4c0). `intro::Cml::from_disc` reads them; equal to the level folders' copies.
Our level folder to course id mapping (`intro::course_of_folder`) is a stand-in for the course table (row C0).

## `.aip` / `.sop`: AI paths and race lines (board row E3d, checked: `tricky-game/tests/aip.rs`, `tricky-data/tests/aip_json.rs`)

- In each course's DATA/MODELS/<course>.BIG (gari, snow, elysium, mesa, merquer, aloha, pipe, untrack, megaple,
  trick, alaska): `<course>.aip` (the race) and `<course>.sop` (the same layout; show-off), compressed.
- `AIP_LoadPathsFromFile` 0x197da0 (`tricky_data::aip::read`): a word it skips (0x0a0a0a0a), the section count,
  sections of (type, size, data). Type 0, AI paths (`AIP_LoadAIPaths` 0x198518, `cAIPath`, 0x40 bytes each): count,
  start-position count, the start positions (path indices; always six). Type 1, race lines (`AIP_LoadEventPaths`
  0x1983d0, `cEventPath`, 0x3c bytes): count, the main line's index. Each path (`AIPath_Load` 0x196f28):
  parameters (id, size, bytes; `cAIPath` 100 = U3 (default 50), 101 = respawnable (default 1); `cEventPath` 0 = the
  distance to the finish), point and event counts, the start, the box (min, max), points of (unit direction,
  length), events of (type, value, start, end), the type kept as its place in the table at 0x337e70 (−1, 0..23,
  100..105, 300).
- The game keeps the points and events where they lie in the loaded file (only the event types rewritten in place).
- Names (F1c, proven): `AIPath_LoadPoint` 0x197130 (the direction, `Vec3_Assign` 0x198720, then the length,
  `Float_Assign` 0x198750: copies of 12 and 4 bytes unless source and target are the same) and `AIPath_LoadEvent`
  0x197170 (the type looked up in the 0x337e70 table, 32 entries, 0 if absent; then value, start, end).
- The app reads the course's `.aip` from the disc with TRICKY_DISC (`aip::from_disc`, `AiFile::from_aip`), else
  the level folder's AIP.json (SSX-Library's export: the same paths, its points direction × length).

## Config files (row E5b; checked: `tricky-game/tests/ini.rs`)

`DATA/CONFIG/*.INF` are read with one reader, `IniReader_*` 0x221668..0x222040 (`tricky_data::ini::IniReader`): a
cursor over the text (a NUL after it) and one token buffer. `#` and `;` comment to the line's end; `[NAME]` heads a
section (`IniReader_NextSection` scans for the next `[`); `KEY = "string"` / `= n` (`sscanf` "%d") / `= x` ("%f"),
the key compared with the last token read (`stricmp`). `IniReader_FindSection` matches a name in any case,
`IniReader_FindSectionMatching` by prefix either way (case matters); `IniReader_SkipToNextSection` also skips the
headers of sections that follow at once, so an empty section's keys are its successor's. The loaders (the pointer
table at 0x343070): `Audio_LoadFEConfig` 0x20f160, `Audio_LoadInGameConfig` 0x211098 (audio, banks, crowd, chant),
`Sfx_LoadSurfaceConfig` 0x210c38 (snow), `Music_StartCourseSong` 0x214e60 (intromus), `MusicSys_Init` 0x224a98
(music), `MusicSys_LoadMusicMap` 0x224cf8 (musicmap), speech 0x3436fc, nascript.

Music (`tricky_data::music`): `MusicSys_Init` reads `[GLOBAL]` BASEPATH, then every other section in order (32 at
most) is a song: its name, the path BASEPATH + LOOPDATA. `MusicSys_LoadMusicMap`(course name upper-cased, from
`Course_GetInfo`) finds the map's section by prefix and enables, for each `SONG = "…"`, the first song whose name
contains it (case matters; four at most); none: song 0. The app plays these (TRICKY_DISC), in the game's song order.

Audio settings (row E5b2, `tricky_data::audiocfg`, checked: `tricky-game/tests/ini.rs`): `Audio_LoadVolumeLevels`
0x21ca20 reads SFXLEVEL, MUSICLEVEL, CHARSPCHLEVEL, PASPCHLEVEL, JUKEBOXLEVEL, UBERLEVEL, INTRODUCK from the scaling
section (IGLEVELSCALING in a race, FELEVELSCALING in the menus) and then from the course's section, searched on from
there (no rewind); a level is the value / 100 when 0..200 (the duck 0..100), else 1.0 (the duck 0.75), into
0x343054..0x34306c. `Music_StartCourseSong` 0x214e60: the course's name lower-cased, the first INTROMUS section
(lower-cased) it contains that is not empty, its first word SONG = "file" → `data/audio/<file>` as the intro stream
(TRICK has none). Then segment 0, one of 1..3 at random (`(rand & 0x7fff)·3/0x7fff + 1`: 4 for 0x7fff), and two of
4..7 (`·4/0x7fff + 4`: 8 for 0x7fff); not in modes 3, 7, 8, 11.


## Course logic and collision, `.ssf` (row E3c1; `tricky_data::ssf`, checked: `tricky-game/tests/ssf.rs`)

- `Course_LoadRenderData` 0x25edf0 builds the course's file names as `|data/models/<file><ext>` (in the course's
  archive): `.ltg`, `.pbd`, `.ssh`, `.ssf`, `.adl`, `.aip` (`.sop` in show-off), and `<file>_L.ssh` (the first six
  letters of a long name, then `_L`) for the lightmaps. `Course_LoadSphereTrees` 0x25f598 reads the `.ssf`; without
  one (or with the wrong version, or an object count that is not the course's) every object gets the default
  physics record 0x3d2d70 (1000.0, 0.5, flags 3 << 16, −1).
- Header (19 words): +0 0x1e0200, +4 version u16 0x1500, +8 a build stamp; then count/offset pairs: +0xc/+0x10
  event slots (7 words: the effect header each of seven events runs, −1 none; `Instance_GetEventScript` 0x25fe78),
  +0x14/+0x18 sphere-tree groups and +0x1c/+0x20 collision groups (offset, bytes, count), +0x24/+0x28 effect headers
  (op count, offset from the end of that table), +0x2c/+0x30 script functions (op count, offset as the headers', a
  16-byte name; `World_FindScript` 0x260dd8 compares case-blind), +0x34/+0x38 physics records (0x18 bytes: +8
  flags, copied to instance+0xe8; +0x14 the event-slot record as a signed half), +0x3c/+0x40 one physics record
  index per object (instance+0xec points at it; flag bit 0x2000 of the upper half sets model+0x10 bit 0x10),
  +0x44/+0x48 spline segments (word 0 to segment+0x18 with its upper half also moved down, word 1 to +0x24).
- A script op: word 0 the op, word 1 its size in bytes (`TriggerScript_Run` 0x13be78 steps by it), the arguments
  after. A collision mesh: triangle count n, vertex count m, padding bytes, n triangles of three u32 indices, the
  padding, m vertices (4 floats), n planes (normal and w); the loader keeps (n, m, triangles, vertices, planes) in
  0x14-byte records (course+0x24) and (first record, count) per group (course+0x20).
- An instance's collision meshes (row E3a5; checked against the levels folders' Collision/*.obj, 3,789 meshes,
  88,828 triangles, all 11 folders, exact): its physics record's collision mode not 3 and group ≥ 0 → that group's
  meshes, each its own OBJ in the exports (numbered by mesh across all groups); each triangle's normal is its plane's.
- On the disc: gari 3393 objects, 306 effect headers, 20 functions, 412 collision groups; ssxfe (the front end)
  has 33 functions and no collision. `untrack` and `trick` have no functions.
- Sphere trees (row E3c2; `Ssf::sphere_trees`): each group's trees follow one another (`SphTree_Load` 0x239340
  returns the bytes it read; the group's byte count is their sum). Header 0x70 bytes: +0 / +4 the sizes of the two
  data blocks after the levels, +8 a flag (0: the occupancy bytes are the data itself, else 0 — the tree's cache
  fills them), +0xc the depth, +0x10 the centre (also the tree's +0x90, w 1), +0x1c the centre of mass, +0x28 and
  +0x4c two 3×3 matrices (inertia and inverse); then depth + 1 level records (12 bytes: sphere radius, cell
  half-size, child index stride 8ⁿ) and the data. The trees are octrees: a node's eight children sit at the
  corners (the tree's +0x10, 8 vectors from 0x3439d0) × the next level's half-size, an occupancy byte saying which
  exist (`SphTree_SphereRecurse` 0x23a728). The loaded tree is 0x150 bytes (header copy at +0xd0, its +0 the tree's
  running index).

## World grid, audio links, skies (row E3e; `tricky_data::ltg`, `tricky_data::adl`, `courses::skies`; checked: `tricky-game/tests/ltg.rs`, `tests/courses.rs`)

- `.ltg` (`cCourseResolve_Common::ResolveTGroupHeader` 0x252470 and on; their debug prints name the fields):
  version (3 bytes) and endianness, the world's box (min, max, centre), the TGroup size (10000 cm), the grid's
  width × height in TGroups, used / empty, the INode size (2500), INodes across (4) and in a TGroup (16), the
  TGroup table's offset (a word per square, row by row, 0 empty) and where the TGroups start. A TGroup: its box,
  counts (patches, instances, splines, lights inside, lights crossing, particle instances, two more, elements), the
  offsets of its index lists into the `.pbd` (resolved to pointers by `Course_GetPatch` / `GetInstance` /
  `GetSpline` / `GetLight` / `GetParticleInstance`), and 16 INodes (0x4c bytes at +0x54: box, seven counts, six
  list offsets into the TGroup's lists). `WorldGrid_CellAt` 0x260ba8 maps INode coordinates to the INode.
- `.adl`: a zero byte, 1.0, a count, then (instance name hash, zone offset) pairs (`AudioTGroups_Bind` 0x22a5f0:
  the hash becomes the instance, whose +0xf4 points back). A zone: a word, a count, shapes (type 0: 7 words, 1 and
  2: 12, 3: 6; 0/2/3 spheres at words 2..5, 1 a box with half-sizes 5..7 along direction 8..10).
  `AudioTGroup_GetBounds` 0x22aaa0 boxes them (moved by the instance's position) for the audio grid.
- Skies: `Course_LoadSkies` 0x185b18 loads `data/models/<course>_sky.pbd` (+ `.ssh`), one per course.
- `.map` files are not read by the game (no such name in the ELF); they are the tools' instance names.

## Level data, `.pbd` (row E3a; `tricky_data::pbd`, checked: `tricky-game/tests/pbd.rs`)

- `cCourseResolve_PS2::ResolveDataHeader` 0x255630 and the resolvers it calls (their debug prints name every
  field). Header: version, format (1 = PS2), 15 counts (player starts, patches, instances, particle instances,
  materials, material blocks, lights, splines, spline segments, texture flips, models, particle models, textures,
  cameras, light map size) and 18 offsets (the same tables, the model / particle model / camera pointer tables, the
  hash table, the VIF block). Records: patch 0x1c0, instance 0x100, particle instance 0x70, material 0x48, light
  0x5c, spline 0x28, spline segment 0x80; material blocks and texture flips are runs of (count, words).
- The resolver turns offsets into pointers and indices into records: instance +0xc0 its model (by id through the
  model pointer table), +0xc4 / +0xc8 other instances, +0xf0 its material block; particle instance +0x40 its model;
  material +0x44 its texture flip; spline +0x20 a segment; segment +0x50 / +0x54 segments, +0x58 a spline; models'
  hierarchies (6-word nodes: parent, three LOD meshes, animation, matrix — −1 the identity at 0x346440), meshes
  (contexts table, each context's VIF data from the VIF block), animations (channels); particle models; cameras;
  the hash table's six lists (patches, instances, models, lights, splines, cameras).
- Naming: the game's "splines" (+0x20 / +0x5c, 0x28 bytes) are what `Course_GetSplineSegment` returns and the
  `.ssf` calls segments; its "spline segments" (0x80) are `Course_GetSpline`'s.
- A quirk: the texture flips' offset (+0x64) is resolved twice, so the header word ends up file + file + offset
  (the flip table is built in between, from the right address).
- Skies (`<course>_sky.pbd`, in the course's archive) are the same format: one model; most courses share one file.
- Fields (row E3a2; `pbd::{PatchRec, InstanceRec, MaterialRec, SplineRec, SegmentRec, LightRec}`, `ssf::Physics`,
  `map::Map`; checked against the levels folders' JSON exports: `tricky-data/tests/pbd_json.rs`):
  - patch: lightmap rectangle, four corner UVs (u, v, 1, 1), the surface as 16 polynomial coefficients (highest
    power first; the exports' Bézier points are converted from them), box, corners, +0x1a8 surface type, +0x1ac bit
    31 show-off only, +0x1b0 texture (low half) and lightmap page (high half).
  - instance: matrix, light directions (columns), three light colours, ambient colour, model id, previous / next,
    box (zero on the disc: filled at run time, row E7a3), material block. Its `.ssf` physics record: +0 mass (1e30 never moves), +4 bounce, the flag defaults (0x1
    shown, 0x20 collides, 0x80 bounces, 0x1000 ?, 0x2000 UV scroll), +0xc surface type, +0x10 collision mode,
    +0x12 the sphere-tree group (mode 3) or collision group, +0x14 effect slot.
  - material: texture (low half), +0x44 texture flip (a run of texture numbers). Spline: box, segment count, first
    segment. Segment: a cubic as four coefficients (highest first), previous / next, spline, box, length. Light:
    type, sprite size, colour, direction, position, box.
  - The `.map` (on the disc beside the others, not read by the game) names everything (and gives the hashes).
  - gari's export in the levels folder is of another build: its splines all differ and its patches' flag word is 0
    there (its show-off patches are surface 18); everything else agrees.
- Meshes (row E3a3; `pbd::{context_strips, strip_triangles}`, checked against the levels folders' Meshes/*.obj:
  8,325 meshes, 313,373 triangles, positions, UVs and normals; proven against the VU1 program in row E3a6,
  `tricky-rs/game/tests/vu1world.rs`). A model's node +4 is its mesh; each mesh context's word 3 (+0xc) is its
  offset into the VIF block (header +0x84), +0x10 is its UV-scroll scratch offset (16 bytes immediately past the
  chain's `ret` tag), and +0x14 / +0x18 are the sentinel `ref` addresses for batch 0 / continuation batches.
  - **Proven (VU1 program 5, chain `0x319e30`, 818 pairs, `BASE 0x18`, `OFFSET 0x1bd`, selected by
    `cPS2GraphicsMan::DrawModel` 0x1e1f70):** `DrawModel` unpacks 11 quadwords to `TOPS+0..10` (4 rows of the
    model matrix pre-scaled by the model's vertex scale `+0x18`, 3 light directions, 3 light colours + ambient) and
    `call`s the mesh context's DMA chain.
  - **`ref` packets (`CourseDma_ResolveMeshContext` 0x256080):** across all 11 courses + skies (3,183 contexts,
    5,408 batches, 10,816 `ref` tags, 0 unknown), every `ref` matches one of the context's three sentinels:
    `+0x14` (3,183, one per context) is replaced at load time with `MSCAL 0x26` (batch 0: `MVP = Clip × Model` via
    subroutine `0x234`, copies the 7 lighting quadwords to `VU1[6..12]`, sets `vi7 = TOP + 11` and falls into
    `0x3e`); `+0x18` (2,225, continuation batches) is replaced with `MSCAL 0x3c` (`vi7 = TOP`, directly into
    `0x3e`); `+0x10` (5,408, one per batch) is replaced inline with `STMOD 1; STROW 0, 0` by
    `CourseDma_PatchContext` 0x240a98 when `model+0x10 & 0x10 == 0` (true of every model on the disc), or kept as a
    relocated `ref` to `chain_base + context+0x10` by `CourseDma_RebuildChain` 0x240c78 so `DrawModel` can write
    `STMOD 1; STROW (int)(du * 4096), (int)(dv * 4096)` each frame (`R2 = 0x1000, R3 = 0x1000` follow in the next
    `cnt` packet).
  - **GS tag and header (`V4-32`, 2 quadwords at `vi7+0..1`):** `vi7+0` is copied verbatim to the output GIF buffer
    (`vi3 = TOP + 0xde`) and kicked via `XGKICK vi3` (`0x8a`). Across all 5,408 batches it is
    `[0x00008000 | NLOOP, 0x302e4000, 0x00000412, 0x00000000]`: `NLOOP = n_verts`, `EOP = 1`, `PRE = 1`,
    `PRIM = 0x05c` (`TRISTRIP` 4, Gouraud `IIP`, textured `TME`, alpha-blended `ABE`, `STQ` coords `FST=0`),
    `FLG = PACKED`, `NREG = 3`, `REGS = 0x412` (`ST`, `RGBAQ`, `XYZ2`). `vi7+1` holds `[n_strips, 0, n_verts, 0]`:
    word 4 (`ilw.x`) is the strip count, word 6 (`ilw.z`) the vertex count for the lighting pass (`bal 0x0`), and
    words 5 and 7 are always 0. `V2-8` strip lengths hold `x = strip_len × 3` (quadwords) and `y = 0` on all
    27,333 strips.
  - **Masked rows (`STCYCL` 1/3, three quadwords per vertex: UV, normal, position):** UVs (`V2-16` masked, `0x75`)
    run under `STMOD 1`, `STROW [du*4096, dv*4096, 0x1000, 0x1000]`, `STMASK 0x50505050` (`x,y` = data + ROW;
    `z,w` = `0x1000` → `itof12` = `1.0`, giving `STQ = (u/W, v/W, 1/W)` after `mulq`). Normals and positions
    (`V3-16` masked, `0x79`) run under `STMOD 0`, `STROW [0, 0, 0, 0x00008000]`, `STMASK 0x40404040` (`x,y,z` =
    sign-extended 16-bit data; `w` = `0x8000` = +32768 → `itof15` = `+1.0`).
  - **Strip winding and ADC bit:** `0x51` loads `vi9 = 0xFFFF` (GS `XYZ2` ADC bit 15 = 1, no drawing kick). Each
    strip starts at `0x5f`: vertex 0 sets `XYZ2.w = 0xFFFF` (`0x6a: mfir.w`), vertex 1 keeps it (`0x73: ftoi4.xyz`),
    and vertices `2..len-1` clear it (`0x80: ftoi4.xyzw`, `ADC = 0`) so the GS draws standard alternating-winding
    `TRISTRIP` triangles (`(i, i+1, i+2)` / `(i+2, i+1, i)`), or sets `ADC = 1` and clips at `0x8c → 0x12b` when
    frustum-crossed. Verified on 1,026 real Elysium contexts (7,556 strips, 52,340 vertices, static and UV-scrolled)
    through `r5900::vu1::Vu1` (`tests/vu1world.rs`). gari is not compared against the JSON export (another build).
- The app from the disc (row E3a4; `Level::from_disc`, checked: `tricky-data/tests/level_disc.rs` against all 11
  levels folders: 28,835 patches, 25,128 instances, 8,325 world and 275 sky meshes, 12,388 collision meshes, 1,540
  splines). Proven: `Course_LoadRenderData` 0x25edf0 mounts `data/models/<file>.big` and reads `<file>.pbd`, `.ssf`,
  `.aip`; `Course_LoadSkies` 0x185b18 reads `<file>_sky.pbd`. The `.map` (names only) is read by no code in the game;
  the port takes the names from it to keep the folders' names. A model node's matrix (+0x14, -1 none) is split into
  position, rotation and scale; its animation keeps the head's six floats, the action word and each channel's cubic
  segments (count at the channel, then the offset to the first). The scripts still come from the levels folder (row
  E9b2); with `TRICKY_SIM=600` elysium and aloha ride the same from the disc as from
  the folder (distance, speed, airtime, falls within a few percent; the folder's numbers are rounded to 6 digits).
- The course's textures from the disc (row E3a7; checked: `tricky-data/tests/level_disc.rs`, 1,710 textures,
  lightmaps and sky textures of all 11 folders, pixel for pixel). Proven: `Course_RemapTextures` 0x2606f8 and
  `Course_RemapLightmapTextures` 0x260898 load an `.ssh` each (`World_LoadTextures`) and replace every material's
  four texture numbers, its flip-book's, and the patches' (+0x1b0, four shorts, kind 1 = texture, 5 = lightmap in
  +0x1ac's 3-bit fields) by the table entry of that number: the numbers are the shapes' order in the file, which is
  what the folders' `<n>.png` are. The files: `<file>.ssh`, the lightmaps' `<file cut to 6 letters>_L.ssh`
  (`Course_LoadRenderData` 0x25edf0: `elysiu_L`), the sky's `<file>_sky.ssh`. The exports kept the lightmaps' stored
  alpha (0-0x80); the app reads only their colour. With the disc the app needs no levels folder: the course list is
  the 11 courses, and a course's folder, if there is one, gives only the scripts.

## The small films (row G11a1; `data/src/video.rs`, checked: `game/tests/video.rs`)

The disc's videos come in two kinds:
- **`.MPC` (the intro, the extras, the characters' films):** they interleave MPEG-2 video (`MPCh` chunks, one picture
  each, 512 × 448) with EA's streamed audio (`SCHl`, `SCCl`, `SCDl`, `SCEl`). `cVideoModule` plays them on the PS2's
  MPEG decoder; that is row G11a2.
- **`.SSS` (the courses' previews, and the trick book's films in `DATA/VIDEO/TB/TB_*.BIG`):** small films that
  `cVideoStreamMan` plays.

An `.SSS` film is a run of chunks, each an 8-byte head (the tag `SSSF`, then the chunk's size with the head) and one
frame. The frame is a shape of type 0x60 (DXT1), 128 × 64 or 128 × 128. The playback goes:
- `VideoStream_Open` 0x1ff990 sets the rate: 60 / the frames a second, kept to 1..30;
- `VideoStreamMan_UpdateFrame` 0x1ffa88 takes the stream's next chunk at that rate;
- `VideoStreamMan_DecodeFrame` 0x1ff928 sets the frame's type byte to 0x60, unpacks it, copies the pixels past the
  16-byte header into the film's texture (+0x94, its size at +0x98) and frees the unpacked shape.

`Shape_Unpack` 0x2c2318 handles the converted types:
- 0x60 is DXT1. 0x61 and 0x62 are the same colour blocks 16 bytes apart (DXT3 and DXT5 with their alpha skipped);
- the output keeps the 16-byte header with its type byte set to `Shape_PixelFormatIndex(0xf)`, which is 3;
- the pixels are 16-bit PS2 ones from `Compress_Decode18Rows` 0x2c2bf8, already ported in `compress.rs`;
- its buffer is the width × 2 (0x60) or × 4 (0x61, 0x62) × the height. For 0x61 and 0x62 only the first half is
  written, and the rest is whatever the allocator gave (the port zeros it, STANDIN).

Checked in the runner on 3,000 random shapes of the converted types and others, through `Shape_Unpack` and
`DecodeFrame`. With `TRICKY_DISC`, every film on the disc decodes: 406 films, 28,894 frames. 8 of 9 mutants are caught
(two by the data crate's unit tests of the chunk walk). The survivor turns 0x62's mode 5 into 3, which the block
decoder treats alike.


## Names from F1c batch 17 (proven)

- `MemCard_ClearCurrentSlotStatus` 0x23ea98: clears +0x174 / +0x178 of the current slot (+0x10, 0x14 apart).
- `MemCard_CheckRoomForFiles` 0x23fcb8: free space (vf 0x8c) and each file's size (vf 0x9c); a file not yet on the
  card gets +0x1a4 + i·0x70 = 1 if it does not fit, else its size comes off the free space.
- `App_GetCourseId` 0x17c360, `App_FindCourseById` 0x17c380, `App_GetCourseInfo` 0x17c3a8: wrappers (no callers)
  on the course table at gApp+100; `CourseTable_GetId` 0x186440: record i's +0x80 id (i capped at 13).
- `App_FreeEffectsLoader` 0x17b938 (gApp+0x30, made by `App_CreateEffectsLoader`), `EffectsLoader_Delete` 0x17c468.
- Not settled: 0x23ed48 (sets a +0x19c flag). (`CourseDma_RebuildChain` 0x240c78 settled in E3a6: rewrites a
  UV-scrolling world mesh's DMA chain, replacing `+0x14`/`+0x18` refs with `MSCAL 0x26`/`0x3c` and relocating `+0x10`.)

## Names from F1c batch 18 (proven)

- Sony's `libmc` (all RPC calls to the card server 0x405580, command kept in 0x35d3a0, semaphore 0x35d3a4):
  `sceMcGetInfo` 0x2f1c00 (1), `sceMcOpen` 0x2f1428 (2), `sceMcClose` 0x2f1598 (3), `sceMcSeek` 0x2f1650 (4),
  `sceMcRead` 0x2f17b8 (5), `sceMcWrite` 0x2f18d8 (6), `sceMcMkdir` 0x2f1560 (`sceMcOpen` with mode 0x40, then
  command 0xb), `sceMcChdir` 0x2f1f68 (0xc; its end callback `Mc_ChdirDone` 0x2f1ee0 copies the current dir out),
  `sceMcChangeThreadPriority` 0x2f12b0 (0x14), `sceMcSync` 0x2f1ac8 (mode 0 waits, polling every 0x3c ticks with
  `Mc_Delay` 0x2f1a80 = `SetAlarm` + `SleepThread`, woken by `Mc_AlarmWakeup` 0x2f1a58). Not settled: 0x2f1368
  (command 0x15, waits for its result), 0x2f1280 (hands out the RPC buffers). The read and info callbacks 0x2f1728 /
  0x2f1ba8 are not functions in Ghidra.
- `MemCard_Process`'s steps use them: 0x16 / 0x25 open to write (mode 0x203), 0x17 write, 0x18 / 0x1e close,
  0x1c open to read (mode 1), 0x1d read, 0x24 mkdir.
- `cPS2MemCard` virtuals: `BeginSave` 0x23dea8 (vf29: dir name into +0xb1, "%s/%s" path into +0xf1, buffer +0x2c,
  size +0x20, step 0x15), `BeginSaveSlot` 0x23df20 (vf31: the same with the slot's file name), `SaveMore` 0x23df80
  (vf33: buffer, size, step 0x17), `EndSave` 0x23dfb8 (vf35: step 0x18, the close); `BeginLoadSlot` 0x23e038 (vf22,
  through `MemCard_BeginLoad` 0x23dfc0: step 0x1b), `LoadMore` 0x23e090 (vf23: step 0x1d), `EndLoad` 0x23e0c8
  (vf25: step 0x1e). `MemCard_GetSlotFileName` 0x23ecf0: the slot's name (+0x1a8 + slot·0x70).

## Names from F1c batch 20 (proven)

- `BIG_FindEntry` 0x2cc0d8 (`BIG_Lookup` with the table at 0x3bb960; the entry's address or 0),
  `BIG_GetFileCount` 0x2cc1b0 (`BIG_Identify` 1: the big-endian count after the magic), `Shape_Find` 0x2c25f0
  (a thunk to `Shape_FindByName`).
- Pad library: `scePadGetButtonMask` 0x2e7e08 (the four mask bytes +0x79..+0x7c when the pad's +0x72 is 1),
  `scePadSetButtonInfo` 0x2e7ec0 (RPC command 10; on 1 `scePadSetReqState` 2), `scePadInfoPressMode` 0x2e7f70 (the
  mask is 0x3ffff), `scePadEnterPressMode` 0x2e7fd0 (`scePadSetButtonInfo` 0xfff). `game/src/inputring.rs` calls
  these two `enter_press_mode` and `press_mode_next`; by the SDK they are the press-mode query and its entry. The
  Rust code was not changed.
- Not settled: 0x2c8068 (a file open through `FileSys_RunWithHeap` 0x2c7f60; `speechload.rs` and `crowdbox.rs` call
  it), 0x2cc2c0 (from `FileSys_InitWithBuffer`: a poll task 0x2cc680 and the mode at 0x3bba60).

## Names from F1c batch 21 (proven)

Sony SDK libraries linked into the game, named after the SDK functions they match. Each was matched by the
RPC command number it sends (or the hardware register it touches) and by its callers in the game.

- **SIF and file I/O (libkernel / sifrpc / fileio):** `sceSifInitCmd` 0x3089b8, `sceSifExitCmd` 0x308c38,
  `SifCmd_IntrHandler` 0x308eb0, `sceSifInitRpc` 0x309088 (boot.rs `SDK_INIT`), `sceSifExitRpc` 0x309228 (called by
  `sceSifRebootIop`), the RPC packet helpers `SifRpc_GetPacket` 0x309318 / `GetFPacket` 0x309348 / `OnEnd` 0x309388
  (packet ids 0x80000009..0x8000000c) / `OnRData` 0x309440 / `FindServer` 0x3095f8 / `OnBind` 0x309648.
  File server (bind 0x80000001, buffer 0x40a640): `sceFsInit` 0x30a588, `sceFsReset` 0x30a7b8, and by RPC number
  `sceLseek` 4, `sceRead` 2, `sceIoctl` 5, `sceIoctl2` 0x1a, `sceMkdir` 7, `sceFormat` 0xe, `sceAddDrv` 0xf,
  `sceDclose` 10, `sceDread` 0xb, `sceGetstat` 0xc, `sceChstat` 0xd, `sceRename` 0x11, `sceSync` 0x13, `sceMount`
  0x14, `sceLseek64` 0x16, `sceDevctl` 0x17, `sceSymlink` 0x18, `sceReadlink` 0x19. `Fio_PathCmd` 0x30b888 sends
  a path with the command in a1; one-line wrappers pass `sceRemove` 6, `sceRmdir` 8, `sceDelDrv` 0x10, `sceChdir`
  0x12, `sceUmount` 0x15; `sceDopen` 0x30bfd0 passes 9 and turns the reply into an fd. The fd table and lock:
  `Fio_InitFdSema`, `Fio_AllocFd`, `Fio_GetFd`, `Fio_InitSema`, `Fio_Lock`, `Fio_Unlock` (0x309fe8..0x30a578).
- **Module loader (loadfile, server 0x40aac0):** `SifLoad_Init` 0x30d7b8 (binds, RPC 0xff), `SifLoad_ModuleCmd`
  0x30dbc8 (`sceSifLoadModule` and 0x30de10 call it), `SifLoad_ModuleBufferCmd` 0x30d980 (RPC 6, under
  `sceSifLoadModuleBuffer` 0x30db88 and `sceSifLoadStartModuleBuffer` 0x30dba8), `SifLoad_ElfCmd` 0x30de30 (under
  `sceSifLoadElf` 0x30df38, a3 = 1), `sceSifLoadIopHeap` 0x30d6c8 (IOP heap server 0x40a6c0, RPC 3).
- **CD (libcdvd):** `sceCdInitEeCB` 0x2e8538 (creates the callback thread `Cd_CallbackThread` 0x2e8478; CdFs_Mount
  calls it with priority 0xb, stack 0x34a5c0, 0x800), `Cd_CallbackHandler` 0x2e83d8, `Cd_InitSemas` 0x2e86b0,
  `Cd_Exit` 0x2e8748 (from `sceCdInit` mode exit: kills the thread, removes SIF handler 0x80000012), `Cd_Delay`
  0x2e8318 / `Cd_AlarmWakeup` 0x2e82f0 (alarm + semaphore sleep, used by `sceCdSync`/`sceCdSyncS`),
  `Cd_CheckNCmd` 0x2e8d00 (N-command server 0x80000595) and `Cd_CheckSCmd` 0x2e9018 (S-command server 0x80000593),
  `sceCdMmode` 0x2e9660 (S-cmd 0x22; boot.rs `CD_MODE`), `sceCdGetToc` 0x2e9908 (N-cmd 4, 0x810-byte buffer),
  `sceCdStop` 0x2e9b98 (N-cmd 7, async with the callback).
- **Pad (libpad, server 0x403c40):** `Pad_SendActData` 0x2e6f48 (DMAs the actuator block, from
  `scePadSetActDirect`), `Pad_SendInit` 0x2e71b8 (cmd 0x10, from `scePadInit`), `scePadEnd` 0x2e7258 (0xf),
  `Pad_GetDmaStr` 0x2e7578 (returns the newer of the two DMA frames; used by every pad reader),
  `scePadSetVrefParam` 0x2e8080 (0xb), `scePadGetPortMax` 0x2e8150 (0xc), `scePadGetSlotMax` 0x2e81b8 (0xd),
  `scePadGetModVersion` 0x2e8220 (0x12, from `scePadInit`).
- **Memory card (libmc, from batch 18's list):** `sceMcFormat` 0x2f20b8 (0x10), `sceMcDelete` 0x2f2180 (0xf),
  `sceMcFlush` 0x2f22a8 (0xa), `sceMcSetFileInfo` 0x2f2360 (0xe), `sceMcRename` 0x2f2538 (0xe with flag 0x10, cmd
  word 0x13), `sceMcUnformat` 0x2f26a0 (0x11), `sceMcGetEntSpace` 0x2f2768 (0x12).
- **MPEG (libmpeg, used by the movie player vf1 0x2ba560):** `sceMpegInit` 0x2e9e70 (resets the IPU DMA
  channels), `sceMpegCreate` 0x2e9f10, `sceMpegDelete` 0x2ea150 (returns 1), `sceMpegGetPicture` 0x2ea190,
  `sceMpegGetPictureRAW8` 0x2ea1d8, `sceMpegGetPictureRAW8xy` 0x2ea220, `sceMpegAddCallback` 0x2ea370.
- **VU and VIF0 debug (libdev):** `sceDevVu0Reset/Pause/Continue/Exec/GetTpc` 0x2e62c8..0x2e6460,
  `sceDevVu1Reset/Pause/Continue/Exec/GetTpc/GetCnd/PutCnd` 0x2e6778..0x2e6ad0, `sceDevVif0Reset/Pause/Continue/
  GetCnd` 0x2e6c18..0x2e6d68, plus `DevVu0_IsBusy` 0x2e62b0, `DevVu_AnyBusy` 0x2e66c0, `DevVu0_GetState`,
  `DevVu1_GetState`, `DevVif0_GetState`, `DevVu0_SaveIntRegs` 0x2e6470, `DevVu0_SaveFloatRegs` 0x2e6550,
  `DevVu_PutQword` 0x2e66f0 / `DevVu_GetQword` 0x2e6730 (VU1 registers through VU0 memory 0x400+). Reset and pause
  set the FBRST bits (VU0 reset 2, break 1; VU1 0x200 / 0x100); only the resets are called by the game (0x1dbad8).

Unclear (batch 21): 0x2f1368 (libmc cmd 0x15), 0x2e8288 (libpad cmd 0x14), 0x2e8e70 (CD N-cmd 0xe, result
compared with 6 in CdFs_Mount, maybe the drive status), 0x2e87c8 / 0x2e8838 (power-off callback set / call),
0x30d8b8, 0x30d948, 0x30e470 (from `_InitSys`), 0x307830..0x308c38 (deci2 / SIO debug console and printf helpers).

## Names from F1c batch 22 (proven)

EA's system library (memory, debug output, threads, timer) and the file library, read from the code.

- **Memory (EAMem):** `EAMem_Zero` 0x2cc938 (`EAMem_Set` to 0), `EAMem_HeapHeaderSize` 0x2cd990 (heap table
  0x34eff8[flags & 0xf] +0x30), `EAMem_BlockHeaderSize` 0x2cc9a0 (that plus the name's length + 1 when the heap keeps
  names, flag 0x100), `EAMem_GetName` 0x2cd9b8, `EAMem_AllocAligned` 0x2ccf50 (passes all of `EAMem_AllocEx`'s
  arguments: name, size, alignment, alignment offset, flags), `EAMem_Alloc0` 0x2ccf98 (as `EAMem_Alloc` but passes 0
  in the sixth argument, which `EAMem_AllocEx` never reads), `EAMem_Resize` 0x2cda18 (resizes a block in place, -1 =
  as large as the free space after it allows; `BankSlot_Load` shrinks banks with it), `EAMem_ClearHeapTable`
  0x2cd250, `EAMem_InitDefaultHeapMax` 0x2cd890 (`EAMem_InitDefaultHeap(0x6000000)`, no callers).
  Heap checks under `EAMem_ValidateHeap` (block magics 'FB' 0x4246 used, 'SB' 0x4253 heap sentinel, error code in
  gp-0x7f44): `EAMem_IsBlockHeader` 0x2cdc60, `EAMem_CheckNeighbours` 0x2cdcc8, `EAMem_CheckBackLinks` 0x2cddc0,
  `EAMem_CheckInHeap` 0x2cde70, `Mem_IsReadable` 0x2ce2c0 (stub: pointer != 0).
- **Debug output:** 8 outputs at 0x350700 (12 bytes: name, handler, enabled bit) and 64 categories at 0x34f2f8
  (8 bytes: name, enabled bit, group bit). `Debug_Message` 0x2ce5e8 (category, format, ...: if the category is on,
  vsprintf and call every enabled output; `Debug_Fatal` sends to category 2), `Debug_InitChannels` 0x2ce4c0
  (outputs 4..7 and categories 16..63 cleared, category 2 and output 2 on), `DebugOut_SetEnabled` 0x2ce6e0 / `Raw`
  0x2ce3c0, `DebugOut_IsEnabled`, `DebugOut_SetHandler`, `DebugOut_Call`, `DebugCat_SetEnabled` 0x2ce800 / `Raw`
  0x2ce3f0 (category 0 = all, 1 = the group), `DebugCat_IsEnabled`, `DebugCat_SetName`.
- **Threads and timer:** `Sys_InitPollTasks` 0x2cebb8 (clears the 16-entry table at 0x403a00), `Thread_Exit`
  0x2cf358 (hands its own stack to the poll task `Thread_FreeExitedStack` 0x2cee88, then ExitDeleteThread),
  `Thread_WakeSleepers` 0x2cf068 (timer callback added by `Thread_SleepTicks`: wakes the 256 sleep slots at
  0x34f5f8 whose tick has come, removes itself when none are left), `Timer_AddCallback` 0x2cf598 /
  `Timer_RemoveCallback` 0x2cf608 (8 slots at 0x350688, run by `Timer_IrqHandler`, which also counts the tick in
  gp-0x7e60), `Fpu_SaveRegs` 0x2cf7e0 / `Fpu_RestoreRegs` 0x2cf8e8 (the handler saves the 32 FPU registers around
  them).
- **Async file loads:** callbacks of `AsyncFile_Load`/`LoadInto`/`StartRead`: `AsyncFile_OnOpened` 0x2c5a98,
  `AsyncFile_OnSized` 0x2c59c0 (allocates through the hook gp-0x7f98), `AsyncFile_OnChunkThenClose` 0x2c58d0 and
  `AsyncFile_OnChunk` 0x2c5b98 (0x8000-byte reads; the first closes the file at the end), `AsyncFile_OnClosed` 0x2c5898.
- **Streams:** `Strm_ClassifyPackets` 0x2c6668, `Strm_OnDataRead` 0x2c6928, `Strm_SetPriorities` 0x2c7220
  (+0x38 normal, +0x3c urgent read priority), `Strm_SetLowWater` 0x2c7270 (+0x40), `Strm_SetUrgent` 0x2c72e0
  (+0x44; raises the read's priority with `FileSys_SetPriority`), `Strm_GetFillBytes` 0x2c7cf0 (+0x48;
  `Strm_ConsumeBytes` turns urgent when it falls under the low-water mark).
- **Sync file calls:** `File_GetSize0` 0x2c7f18, `File_LoadAlloc0` 0x2c8068, `File_LoadInto0` 0x2c8348,
  `File_OpenBig0` 0x2c86b8: the same as `File_GetSize`, `File_LoadAlloc`, `File_LoadInto`, `File_OpenBig` but they
  store 0 instead of 1 in word +0xc of the argument block, which neither the callbacks nor `FileSys_RunWithHeap`
  read. Note: `speechload.rs` calls 0x2c8068 `OPEN_BIG` and `crowdbox.rs` `OPEN_FILE`; its callback is
  `File_LoadAllocCb` 0x2c7f60 (load the whole file into new memory). `File_PeekUnpackedSize` 0x2c88d8 (reads the
  first 0x80 bytes, returns `Compress_UnpackedSize`; 0x2c8910 is the same without the read check,
  `File_PeekUnpackedSizeUnchecked`), `File_LoadUnpackInto` 0x2c8a90 (reads, unpacks if packed, copies at most n
  bytes, then flushes the caches; `File_LoadUnpackInto2` 0x2c8bc8 is a jump to it, called by the lesson handlers'
  Enter), `File_WriteSync` 0x2c8e28 (`File_ReadChunked` with `FileSys_QueueWrite`, as `File_ReadSync` passes
  `FileSys_QueueRead`).
- **Disc and queue:** `CdFs_CdCallback` 0x2c9498 (given to `sceCdCallback`), `CdFs_StartRead` 0x2c9e20 (from
  `FileSys_DevRead`: splits a read into whole and part sectors), `CdFs_RecoverRead` 0x2c9710 (re-inits the drive,
  rereads sector 0x10, then the pending read) and `CdFs_PollRecover` 0x2c9930, `FileSys_SetPriority` 0x2ca460
  (op +0x10, re-sorts the queue), `FileSys_GetOpState` 0x2ca3b0 (+0xc), `FileSys_GetOpContext` 0x2ca3e0 (+0x14),
  `FileSys_OnBigOpened` 0x2cadc0 / `FileSys_OnBigClosed` 0x2cafa8 (callbacks of the BIG mount and unmount),
  `FileSys_SetHeap` 0x2cb1f0, `FileSys_ParsePath` 0x2cb830 (splits `device|path`), `HostFs_Init` 0x2cc2c0
  (adds `HostFs_ServiceRead` as a poll task). `disc.rs` says 1000 files is "0x2cc2c0's default"; 0x2cc2c0 has no
  such number (left for the owner of disc.rs).

Unclear (batch 22): 0x2cf2f8 (ReferThreadStatus, result unused), 0x2c7098 (stream call from the movie player),
0x2c73a0 / 0x2c7de8 (request word +4), 0x2c7d20, 0x2c84a0, 0x2c8ef8 / 0x2c9070 (sync ops through the stub 0x2cebb0,
which always returns 0), 0x2cac38 (queues op type 7), 0x2cb9c8 (empty), thunks 0x2cc328 / 0x2cc348.

## Names from F1c batch 23 (proven)

Host code linked into the exe: gcc 2.95's C++ runtime (libgcc2 `L_eh`, `frame.c`, `exception.cc`,
`tinfo.cc`/`tinfo2.cc`, `fp-bit.c`), newlib and Sony's libkernel, plus a few libmpeg helpers. Each was read in
Ghidra and matched to the library's code by its structure, its callers and the fields it touches; the names are
the libraries' own (static ones included).

- **Exception throwing (libgcc2):** `old_find_exception_handler` 0x2f4998 (3-word {start, end, handler} table
  ending in -1), `find_exception_handler` 0x2f4a70 (4-word entries, match function per entry),
  `get_reg_addr` 0x2f4bb8 (follows saved-in-register state 2, offset state 1 from the CFA, else `abort`),
  `copy_reg` 0x2f4c40, `next_stack_level` 0x2f4cc0 (calls `__frame_state_for`), `__unwinding_cleanup`
  0x2f4d50 (empty), `throw_helper` 0x2f4d58 (called by `__throw` / `__rethrow`).
- **`exception.cc`:** `terminate` 0x2f67b0 (calls the handler at 0x35d3ac, initially `__default_terminate`),
  `__default_unexpected` 0x2f67c8 (the initial value of 0x35d9a4; calls `terminate`), `set_terminate` 0x2f67d8
  and `set_unexpected` 0x2f67e8 (swap 0x35d3ac / 0x35d9a4), `unexpected` 0x2f67f8, `__cp_exception_info` 0x2f6810
  (&info->value, +8), `__cp_eh_info` 0x2f6838, `__start_cp_handler` 0x2f6858 (caught +0x14 = 1, handlers +0x20
  += 1), `__eh_alloc` 0x2f6890 (malloc, `terminate` on failure), `__eh_free` 0x2f68b8, `__cplus_type_matcher`
  0x2f68d8 (language 4 = C++), `__cp_push_exception` 0x2f6948 (0x30-byte cp_eh_info: value +8 and +0x28, type
  +0xc, cleanup +0x10, next +0x18), `__cp_pop_exception` 0x2f69d8, `__uncatch_exception` 0x2f6aa0,
  `__check_eh_spec` 0x2f6ad0 (throws `bad_exception` when the spec allows it).
- **`frame.c` (DWARF2 unwind tables):** `fde_merge` 0x2f7b80, `end_fde_sort` 0x2f7c80 (aborts on count
  mismatch; heapsort inlined), `count_fdes` 0x2f7ee8, `add_fdes` 0x2f7f30, `frame_init` 0x2f7fb8, `find_fde`
  0x2f80f8 (object list at 0x406bd0), `extract_cie_info` 0x2f81e0 (augmentation "" / "eh" / 'z'),
  `execute_cfa_insn` 0x2f82e8, `__frame_state_for` 0x2f87c8, `fde_split` 0x2f89a0.
- **RTTI (`tinfo.cc` / `tinfo2.cc`):** `type_info::OperatorEq` 0x2f8d98 (`operator==`: same object or `strcmp`
  of the names), `type_info::OperatorNe` 0x2f93b8 (`operator!=`), `type_info::Before` 0x2f9438 (`before`:
  `strcmp` < 0), `__throw_type_match_rtti_2` 0x2f9460, `__is_pointer` 0x2f9928, `__dynamic_cast` 0x2f9a40,
  `__tfv` 0x2f9af0 (type info of `void`: name "v", `__builtin_type_info` vtable). The three operators carry
  CamelCase method names because the name check takes no `operator` names.
- **fp-bit (float):** `_fpadd_parts_f` 0x2f70a8 (float copy of `_fpadd_parts`, called by `__addsf3` /
  `__subsf3`; `_f` added by us to tell it from the double one at 0x2f5c88), `__mulsf3` 0x2f73a0 (between
  `__subsf3` and `__divsf3`; no callers).
- **newlib:** `std` 0x2fa4f0 (FILE setup with `__sread`/`__swrite`/`__sseek`/`__sclose`, from `__sinit`),
  `__sfmoreglue` 0x2fa548 (n × 0x58-byte FILEs), `__sfp` 0x2fa5b8, `__sprint_vfi` 0x2fea28 / `__sbprintf_vfi`
  0x2fea70 (`vfiprintf.c`'s copies of `__sprint` / `__sbprintf`, which are at 0x2ff770 / 0x2ff7b8; `_vfi`
  added by us), `quorem` 0x301ec0 (dtoa's Bigint quotient, called by `_dtoa_r`).
- **Sony libkernel:** `_SyncDCache` 0x306f88 (D-cache `DXWBIN` loop over a range) under `SyncDCache` 0x307030
  (DIntr/EIntr around it, start and end masked to 64 bytes; called by libpad), `_InvalidDCache` 0x3070c8
  (`DXIN`) under `InvalidDCache` 0x307170, `iWakeupThread` 0x307718 (wakes another thread with
  `_iWakeupThread`; the current thread is queued instead), `iRotateThreadReadyQueue` 0x3077b0 (queued, priority
  < 128), `TopThread` 0x307568 (the thread draining that queue: op 0 `WakeupThread`, 1
  `RotateThreadReadyQueue`, 2 `SuspendThread`), `InitThread` 0x307640 (from `_InitSys`: makes the semaphore and
  that thread, priority 1). `InitThread` / `TopThread` are ps2sdk's names for the same code (Sony's own
  unknown; ps2sdk has `topThread`, made `TopThread` for the name check).
- **libmpeg helpers:** `Mpeg_RunCallback` 0x2ea398 (calls the callback stored for a type at mp+0x40, +0xc +8k),
  `MpegWork_Init` / `_Mark` / `_Release` / `_Alloc` 0x2ea4b8 / 0x2ea4d0 / 0x2ea4e0 / 0x2ea4f0 (the work area
  bump allocator; `_Alloc` aligns and fails with "work area size is too small"), `Mpeg_GetPictureCommon`
  0x2ea570 (shared body of the three `sceMpegGetPicture*`), `Mpeg_StopDma` 0x2eab28 (stops the IPU_FROM, IPU_TO
  and SPR_TO DMA channels), `Mpeg_PrintError` 0x2eabf0 ("[MPEG ERROR]%s\n", from `Mpeg_Error`).
- Made into functions in Ghidra this batch: 0x2f67d8, 0x2f67e8, 0x2f6810 (were tails of the noreturn
  functions before them). They were added by hand to `function-index.csv` and `function-systems.csv` as
  `runtime`.
- **Unclear (batch 23):** the libkernel newlib stubs 0x306df8, 0x306f38, 0x306f60 (each sets errno 5 and
  returns -1; which newlib call each one is cannot be told apart); 0x306c50 (from `sceSifSyncIop`, clears
  0x35ee70 then jumps to 0x307f08); 0x306ca0 (no callers, waits on the vsync-start bit of I_STAT); 0x307438
  (syscall 0x5a) and its caller 0x307490 (from `_InitSys`, reads T3_MODE); the other libmpeg internals
  (0x2ea158, 0x2ea2a8, 0x2ea2c8, 0x2ea308, 0x2ea3e8, 0x2ea6f8..0x2f0e90).

## Names from F1c batch 24 (proven)

The debug link to the host PC (`CCommDriver` and its Deci2 driver, 0x2b6ae0..0x2b7e78) and the `Cstring` helpers:

- `CCommDriver::CCommDriver` 0x2b6fd0: stores `CCommDriver::vtable` at +0x1c8, clears the counters and builds two
  arrays of ten `Cstring`s (+0x38 and +0x100). No callers (an out-of-line copy).
- `CommStack_Resize` 0x2b70c0 (`realloc` the data to n words; +0 count, +4 data, +8 the last word),
  `CommStack_Pop` 0x2b7118 (resize to n−1), `CommStack_Clear` 0x2b7178 (resize to 0), `CommStack_Delete` 0x2b7128
  (clear, then `Mem_Delete` when bit 0 of the flag is set; from `~CCommDriver`). `CommStack_ResizeCopy1` 0x2b79a0 is
  the same code as `CommStack_Resize`, with no callers. The Deci2 driver's vf6 pushes a 0 on both stacks, runs
  `Deci2Comm_PumpReceive` until the top word is set, then pops both.
- `CommPacket_SetHeader` 0x2b7530: fills a 12-byte header (+0 short, +4 short, +6 byte, +7 byte, +8 word, +2 = 0);
  from the Deci2 driver's vf2 and vf5 (its sends).
- `Deci2Comm_HasPacket` 0x2b7700: the received byte count 0x3d37d4 > 0. `Deci2Comm_PollTask` 0x2b7848: the same as a
  bool; the driver's vf1 (`sceDeci2Open(0xe001, …, Deci2Comm_Handler)`) adds it with `Sys_AddPollTask`.
- `Deci2Comm_ReceiveRecord` 0x2b7550: `Net_ReceivePacket(buf, 0x4d8)`, from `CommDriver_RecvPacket`.
  `Deci2Comm_PumpReceive` 0x2b7570: `Net_ReceivePacket` into a 0x2800-byte stack buffer (the packet is handled
  through the driver's virtuals inside `Net_ReceivePacket`).
- `Cstring_InitInt` 0x2b81c0 (`sprintf "%d"`, then `Cstring_AssignAscii`), `Cstring_ToInt` 0x2b93b0 (`atoi` of an
  8-bit copy), `Cstring_ToFloat` 0x2b9400 (`atof`, to float), `Cstring_GetBinarySize` 0x2b9598 (the length word at the
  head of the buffer, 0 without one; `Cstring_GetBinary` copies that many bytes after it).

## Names from F1c batch 27 (proven)

The kernel's system call stubs (`li v1, n; syscall; jr ra`) that Ghidra's Emotion Engine extension names, put in
`ghidra/symbols.txt` after checking each number: 0x3063e0 `SetGsCrt` (2) .. 0x306c10 `Deci2Call` (0x7c), 52 stubs,
plus `RFU091` 0x307480 (0x5b) and the two copies of `RFU116_SetSyscall` (0x74) at 0x307428 and 0x30e460. Interrupt
context versions use the negative number (`iSignalSema` −0x43, `isceSifSetDma` −0x77, ...). `SetAlarm` 0x306560
uses 0xfc. Two numbers have two stubs each: 0x10 at 0x3064c0 / 0x3064d0 and 0x12 at 0x3064f0 / 0x306500; the first
of each pair is called with three arguments (`Timer_Init`, `Gfx_InstallVif1Handler`, `sceSifInitCmd`), the second
with four (the render setup 0x1dbad8 and 0x182430 pass the handler's argument), so they are `AddIntcHandler` /
`AddIntcHandler2` and `AddDmacHandler` / `AddDmacHandler2` (Sony's names; Ghidra already has the `…2` labels there).
`game/src/boot.rs` `ADD_INTC` is 0x3064d0, `AddIntcHandler2`. Also `entry` 0x100008.

## Names from F1c batch 28 (proven)

- `App_VSyncIntcHandler` 0x182170 (cause, arg): calls `arg`, then `SYNC`, `EI`, returns 1; `boot.rs` adds it with
  `AddIntcHandler2`(3, 0x182170, 0, `App_OnVSync` 0x1825d0). Settles the batch 18 gap at 0x182170.
- `TrickDef_Unload` 0x1545b8: clears the loaded flag 0x31e08c that `TrickDef_Load` 0x154580 sets (called from the
  game's teardown).
- `Score_GetTrickName` 0x157978: score + 0x250, the text `Score_AnnounceTrick` formats the trick's name into
  (`lesson.rs` `trick_text_nonempty`, `LessonOverlay_DrawTipPage`).
- `Rand_Init` 0x2401a0: copies the 0x18-byte default state at 0x3a4a00 into a random generator (the size
  `Rand_SaveSnapshot` writes); from `StaticInit_MemCard`.
- `Vec4_ScaleCopy21`..`31` (0x1780b8, 0x197d70, 0x1cae48, 0x1ce058, 0x1d8dc0, 0x1ea358, 0x201098, 0x208628,
  0x238a80, 0x23b588, 0x2b6a70) and `Mat4_TransformVec4Copy1` 0x2085e8: more out-of-line copies of the VU0
  scale (44 bytes) and `Mat4_TransformVec4` 0x178078 (64 bytes), no references.

## Names from F1c batch 29 (proven)

- `sceDmaPutEnv` 0x2e5950 (env): libdma's environment set: checks the fields (−1..−4 when out of range), then
  D_CTRL (bits from tables 0x35ade8 / 0x35adf8 / 0x35ae08, the release cycle), D_PCR, D_SQWC, D_RBOR,
  D_RBSR, and keeps a copy at 0x35ae18. `sceDmaReset` 0x2e5860 calls it on a zeroed 0x14-byte env.
- `Dma_Bzero` 0x2e5800 (p, n): n bytes zeroed (libdma's own; Sony's name unknown).

## Names from F1c batch 30 (proven)

- `CourseTable_Construct` 0x1861f0: calls `CourseTable_Destruct` 0x186348 (*p = 0) and returns this; caller
  `App_LoadFonts` 0x17b748.
- `AsyncBdrLoad_SetMode` 0x201788 (p, v): *p = v. The front end's `cFE3D` vf1 0x263b30 sets it to 1 at 0x263fd0 on
  its new loader (`AsyncBdrLoad_Init` 0x201b50 clears it); what 1 means is inferred.
- `BdrLoad_FEBoardRequest` 0x201ad8: byte 0 = 1, request word = (w & 0xffffe6ff) | 0x2600: bits 9 (board .mpf),
  10 (board .ssh) and 13 set, 8, 11, 12 (body/head .mpf, suit/boot, head/helmet .ssh) cleared, as the bits of
  `BdrLoad_RequestRider` 0x201790. Caller `cFE3D` vf12 0x266df8 at 0x266e4c (fe_scene.rs LOAD_OPTIONS).
- `AsyncBdrLoad_SetManagers` 0x201c10 (p, a, b, c), no callers: +8, +0xc, +0x10 = a, b, c, the managers
  `AsyncBdrLoad_FreeSlot` 0x201c20 frees through (rider meshes, board mesh, textures; vtable slot 0x5c).
- Sony's IOP side (host): `sceSifGetSreg` 0x308970 (0x407a80[i]); `SifRpc_AllocPacket` 0x309250 (DIntr, first free
  of the rpc's 0x40-byte packets, +0x10 = (i << 16) | 5, request id counter skipping 0) / `SifRpc_FreePacket`
  0x3092f8; `Fio_Intr` 0x30a140 (the fileio RPC receive handler `sceFsInit` registers: copies the tail bytes of a
  read, dread, getstat and the ≤ 0x400-byte replies of commands 0x17 / 0x19 / 0x1a, then `iSignalSema`);
  `Fio_VersionMismatch` 0x30a728 (from `sceOpen`: the IOP's version at 0x40a668 against "2240" and "....");
  `SifLoad_GetIopVal` 0x30df80 / `SifLoad_SetIopVal` 0x30e070 (loadfile RPC 3 / 2, byte / half / word).
- The Deci2 TTY (strings "TTY: ..." 0x3bb208..0x3bb260, protocol 0x210): `sceTtyInit` 0x307f08
  (`sceDeci2Open(0x210, 0x407550, sceTtyHandler)`), `sceTtyHandler` 0x307b50, `sceTtyWrite` 0x307ce8 (from
  `write`; \n → \r\n, ≤ 0x100 bytes), `sceTtyRead` 0x307e38 (from `read`; up to \n or \r), the ring at 0x407440
  {size, count, read, write, data} (`TtyRing_Init` 0x307aa8, `_Push` 0x307ad0, `_Pop` 0x307b10), `Tty_Reset`
  0x306c50 (from `sceSifSyncIop`), `Intc_WaitVblankStart` 0x306ca0 (no callers; waits INTC_STAT bit 2).
- Kernel printf: `Sio_PutcRaw` 0x307fc8 (waits SIO_ISR & 0x8000, writes TXFIFO), `Sio_Putc` 0x3080b0 (\n → \r\n,
  the default putc hook at 0x35f620), `Kprintf_Vformat` 0x3082e0 (fmt, va, through the hook), `Kprintf_Printf`
  0x3088a8, `Kprintf_PutcBuffered` 0x308000 (line buffer 0x407800, flushed at \n or 126 chars; `scePrintf`
  0x3088e0 swaps it in), `Kprintf_PrintDouble` 0x308178 ("0.%d" and "e+%d" / "e%d"), `Kprintf_DoubleToInt`
  0x3080e8 (rounded, 0 if tiny, 9999 if too big).
- libgcc's 64-bit division: `__divdi3` 0x30e4d8 (signed, negates at entry; used by the kernel printf; was named
  `__divdi3_2` here, corrected in batch 31), `__moddi3` 0x30edb0, `__umoddi3` 0x30f418 (callers `_strtol_r`,
  `_strtoul_r`, `_vfprintf_r`, `_vfiprintf_r`, `Kprintf_Vformat`). The old `__divdi3` 0x2f54e8 is `__udivdi3`
  (renamed in batch 31).

Still unclear: 0x1cf648 / 0x1cf698 (no callers; dispatch through the memory card overlay's +0x428 vtable), the
errno stubs 0x306df8 / 0x306f38 / 0x306f60 (errno 5, −1), 0x307438 / 0x307490, libmpeg 0x2ea820..0x2f0e90, and the
lib-snd runs 0x2d6cc0..0x2da7e0 and 0x2e1760..0x2e3620.

## Names from F1c batch 31 (proven)

- libgcc fix (both in `ghidra/renamed.txt`): 0x2f54e8 `__divdi3` → `__udivdi3` (no sign handling at all: an
  unsigned 64-bit quotient; callers `Kprintf_Vformat`, `SndVoice_UpdatePitch`, `_malloc_trim_r`, `_strtol_r`,
  `_strtoul_r`, `_vfprintf_r`, `_vfiprintf_r`), and 0x30e4d8 `__divdi3_2` → `__divdi3` (bgez a0 / negate at
  entry). One libgcc, not two copies: `__udivdi3` 0x2f54e8, `__divdi3` 0x30e4d8, `__moddi3` 0x30edb0, `__umoddi3`
  0x30f418.
- `Kernel_Copy` 0x307438: the kernel syscall 0x5a (Sony's `kCopy`; the name check takes no lower-case-first
  CamelCase, so it is `Kernel_Copy` here). `Kernel_ApplyPatch` 0x307490 (from `_InitSys` 0x30e4b0): when
  RCNT3_MODE & 0x100 is clear, copies 0x740 bytes from 0x35ee78 to 0x80076000 and 0x20 bytes from 0x35f5b8 to
  0x82000 with `Kernel_Copy`, around `RFU116_SetSyscall` and `FlushCache(0)` / `(2)`, then sets syscalls 2..7 again
  from the table at 0x35f5e8 (RFU091 lookup + `RFU116_SetSyscall`). That this is the old-kernel patch is
  inferred.

Still unclear: 0x1cf648 / 0x1cf698 (no callers; through the memory card overlay's +0x428 vtable: +0x44 < 1 →
slots 0xc0 / 0xc4 else 0x78 / 0x7c; +8 == 0 → 0xb0 / 0xb4 else 0x88 / 0x8c), the newlib errno stubs 0x306df8 /
0x306f38 / 0x306f60 (errno 5, −1, no callers; probably open/unlink/link-style stubs, not provable), libmpeg
0x2ea820..0x2f0e90.

## Names from F1c batch 32 (proven)

- A DECI2 host link object with no callers and no vtable found (dead code; names partly inferred):
  `Deci2Host_Destroy` 0x2b7b18 (deleting destructor: `~CDeci2CopydataCommDriver` on +8), `Deci2Host_Open` 0x2b7b78
  (+0x2c / +0x34 / +0x3c = self, +0x28 / +0x30 / +0x38 = the callbacks 0x2b7be0 / 0x2b7be8 / 0x2b7bf0, then the
  driver's vf1 0x2b71d0: `sceDeci2Open` protocol 0xe001 and `Sys_AddPollTask` 0x2b7848; +0x1e8 = 1, returns 1),
  `Deci2Host_Wait` 0x2b7c08 (driver vf6 0x2b7488 sends, then pumps `Deci2Comm_PumpReceive` until the reply flag,
  returns the result).
- Still open: libmpeg 0x2ea820..0x2f0e90 (SDK internals); batch 33 named part of it, below.

## Names from F1c batch 33 (proven)

libmpeg, Sony's MPEG-2 decoder on the IPU, as the movie player (vf1 0x2ba560, `VideoPlayer_FeedMpeg` 0x2ba438)
drives it. Field names follow the MPEG-2 standard (ISO 13818-2), matched by bit widths and start codes; Sony's own
error strings call the parts `_sequenceScalableExtension` and so on.

- **Feed and reset:** `Mpeg_FeedData` 0x2ea158 (buffer → 0x35c910, size rounded up to 16 → 0x35ca40, then
  `Mpeg_StartIpuToDma` 0x2eac88: chain tags at 0x404d40 (uncached 0x20404d40), at most 0xfff40 bytes each, tag id 3
  (ref) or 0 (refe) for the last; IPU_TO TADR = 0x404d40, QWC 0, CHCR 0x105). `Mpeg_Reset` 0x2ea2c8 (from
  `sceMpegCreate`: sys+0/+4/+8 = 0, mp+8 = 0, sys[0x20] = −1, sys[0x2b] = 0, `Mpeg_StopDma`, picture counter
  0x35ca44 = 0, `Mpeg_SetMpeg1Mode` 0x2eaa40 (0x35d1f0 = 0, `Ipu_SetMp1` 0x2ed398: IPU_CTRL bit 23)).
  `Mpeg_ClearRefBuffers` 0x2ea308 (+0x28 = 0 on the six image buffers 0x35c940..0x35c95c), `Mpeg_InitWorkBuffers`
  0x2eaad8 (work 0x35c900 split at +0, +0x1800, +0x1b00, +0x3300), `Mpeg_IsEnd` 0x2ea2a8 (sys+0, set at sequence
  end; read by the movie's vf3 0x2ba810), `Mpeg_SetPictureSize` 0x2ead60 (w, h and both >> 4).
- **Callbacks:** `Mpeg_RunNodataCallback` 0x2ea3e8 (`Mpeg_RunCallback` type 1, run by every IPU wait every 5000
  spins). `sceMpegCreate` stores the default StopDMA at sys+0x1c (type 2) `Mpeg_DefaultStopDmaCb` 0x2eb610 and
  RestartDMA at sys+0x24 (type 3) `Mpeg_DefaultRestartDmaCb` 0x2eb620 (both created; they jump to 0x2f0b88 /
  0x2f0c70 with sys+0x4c). `Mpeg_NextPicture` runs type 5 (time stamp) with pts / dts −1.
- **IPU bit reader:** `Ipu_GetBits` 0x2eeb80 (FDEC 0x40000000 | n, returns the top n bits), `Ipu_ShowBits` 0x2ee930,
  `Ipu_SkipBits` 0x2eea68, `Ipu_WaitCmdResult` 0x2ee6c8, `Ipu_DecodeVlc` 0x2ee778 (VDEC 0x30000000 | table << 26;
  reads IPU_TOP and IPU_BP).
- **Headers:** `Mpeg_NextStartCode` 0x2eece8 (byte-align, skip to 0x000001), `Mpeg_NextPicture` 0x2eedc0 (0x1b3
  sequence header → 0x2ead80, 0x1b8 → `Mpeg_ParseGopHeader` 0x2ef3b0, 0x100 → `Mpeg_ParsePictureHeader` 0x2eeee8 and
  returns picture_coding_type, 0x1b7 sequence end → 0), `Mpeg_ParseExtensions` 0x2eefb8 (0x1b5: 4-bit id into the
  table 0x35d280 of 11; 0x1b2 user data skipped), `Mpeg_SkipExtraInfo` 0x2ef298, `Mpeg_ParseSliceHeader` 0x2eed50,
  `Mpeg_CalcPictureNumber` 0x2ef2d0 (running number from temporal_reference, 0x400 wrap). Extensions by id:
  1 `Mpeg_ParseSequenceExt` 0x2eb3c8 (MPEG-2 mode, `Ipu_SetMp1(0)`; profile/level 0x48 or 0x58 and chroma 4:2:0
  only), 2 `Mpeg_ParseSeqDisplayExt` 0x2eb530, 3 `Mpeg_ParseQuantMatrixExt` 0x2ef498 (created; intra / non-intra
  matrices via IPU SETIQ 0x50000000 / 0x58000000, chroma matrices are an error), 4 `Mpeg_ParseCopyrightExt`
  0x2ef638, 5 `Mpeg_SeqScalableExtUnsupported` 0x2eb5d0, 7 `Mpeg_ParsePictureDisplayExt` 0x2ef548, 8
  `Mpeg_ParsePictureCodingExt` 0x2ef068 (f_codes; intra_dc_precision, q_scale_type, intra_vlc_format,
  alternate_scan into IPU_CTRL bits 16-17, 22, 21, 20; picture_structure → 0x35caec), 9
  `Mpeg_PicSpatialScalableExtUnsupported` 0x2eb5f0, 10 `Mpeg_PicTemporalScalableExtUnsupported` 0x2eb600, 0 and 6
  `Mpeg_UnknownExt` 0x2eb5e0 (the last four created; each only `Mpeg_Error`s its string).
- **Decode:** `Mpeg_DecodePicture` 0x2ea820 (no callers found; frame pictures (structure 3) →
  `Mpeg_DecodeFramePicture` 0x2ea6f8, fields → `Mpeg_DecodeFieldPicture` 0x2ea860).
- Still open in libmpeg after batch 33: named in batch 34, below.

## Names from F1c batch 34 (proven)

The rest of libmpeg. Behaviour is read from the code; the split into functions follows the MPEG-2 reference
decoder's shape (`motion_vectors`, `decode_motion_vector`, `Dual_Prime_Arithmetic`, `form_predictions`), matched
by what each does, not by any name in the binary. Globals: 0x35caec picture_structure (1 top, 2 bottom, 3 frame),
0x35cabc picture_coding_type (1 I, 2 P, 3 B), 0x35ca68 / 0x35ca6c width / height in macroblocks, 0x35ca48 the
"error, resync at the next slice" flag, 0x35d1b8 which of the two macroblock work slots (0x140 bytes each, from
0x35cf38) is in use, 0x35ca5c "waiting for the second field".

- **Sequence and buffers:** `Mpeg_ParseSequenceHeader` 0x2ead80 (from `Mpeg_NextPicture` on 0x1b3: 12-bit width /
  height → 0x35ca60 / 0x35ca64 ("vertical size > 2800" if over 0xaf0), aspect 0x35ca70, frame rate 0x35ca74, bit
  rate 0x35ca78, vbv 0x35ca7c, constrained 0x35ca80; the two load_*_quantiser_matrix bits: 1 = IPU SETIQ reads the
  matrix from the stream, 0 = `Ipu_LoadQuantMatrixDma` 0x2eb308 sends the default from 0x35c9c0 / 0x35ca00 (4 qw on
  IPU_TO, then SETIQ 0x50000000 / 0x58000000); then `Mpeg_ParseExtensions`; MPEG-1 forces frame / progressive; on a
  size change frees and reallocates three 0x1800-per-macroblock buffers at sys+0xfc/+0x100/+0x104 and
  `Mpeg_SetImageBuffers` 0x2eb220 points the nine image descriptors 0x35cb90..0x35ced0 at them (uncached; the
  bottom-field ones at +0x180 per 512 pixels), each sized by `Mpeg_SetPictureSize`).
  `Mpeg_SetupPictureBuffers` 0x2ef880 (I/P pictures swap the forward / backward references 0x35c940..0x35c95c, B
  pictures decode into 0x35c948 / 0x35c954 / 0x35c960; the current targets go to 0x35cb84 (frame) / 0x35cb88 (top) /
  0x35cb8c (bottom); copies the picture's type, structure, time stamps into the target descriptor +0x18..+0x60;
  returns whether the references it needs are decoded (+0x28 == 1)).
- **Picture decode:** `Mpeg_DecodePictureData` 0x2ef6e8 ("odd number of field pictures", "unknown picture
  sutructure"; on success marks the target +0x28 = 1) → `Mpeg_DecodeSlices` 0x2ed860 (resets the work slot,
  repeats `Mpeg_DecodeSlice` while it returns 1 (error, resync) or 3 (slice ended); "= Skip to the next picture ="
  on 1 / 2) → `Mpeg_DecodeSlice` 0x2edab0 (returns 0 picture done, 1 error, 2 fatal, 3 next slice; "Too many
  macroblocks in picture"), `Mpeg_StartSlice` 0x2ed988 (slice_start_code 0x101..0x1af, else "slice_start_code(0x%08x)
  out of range"; first macroblock address from slice_vertical_position, quantiser, `Mpeg_GetMbAddrInc`; clears the
  eight PMVs; on error prints "_sliceA0(): error happens", so Sony's own name is probably `_sliceA0`, inferred),
  `Mpeg_GetMbAddrInc` 0x2ed750 (VLC table 0; 0x22 stuffing (and the MPEG-1 11-bit 0xf), 0x23 escape +33; anything
  else invalid: "Invalid macroblock_address_increment code").
- **Macroblock:** `Mpeg_DecodeMacroblock` 0x2edd80 (picture_coding_type into IPU_CTRL bits 24-26; macroblock_type by
  VLC table 1 ("Invalid macroblock_type code: 0"); frame/field_motion_type, dct_type, quantiser_scale_code 0x35cb80;
  forward / backward vectors (MPEG-1 → `Mpeg_MotionVector` with full_pel 0x35cac4 / 0x35cacc and f 0x35cac8 /
  0x35cad0; MPEG-2 → `Mpeg_MotionVectors` with f_codes 0x35cad8..0x35cae4); concealment marker bit; coded or intra
  starts IPU BDEC 0x20000000 (intra << 27, DCR 0x35cb7c << 26, dct_type << 25, qsc << 16) into the slot's buffer by
  IPU_FROM 0x30 qw; resets PMVs on intra without concealment and in P pictures without forward motion).
  `Mpeg_SkippedMacroblock` 0x2edcb0 ("skiped macroblock in I picure is not allowed"; P zeroes the PMVs).
  `Mpeg_MotionVectors` 0x2ee330 (one vector with optional field select, or two field vectors),
  `Mpeg_MotionVector` 0x2ee4b0 (motion_code by VLC table 2, residual `Ipu_GetBits(r_size)`, optional dmvector for
  dual prime), `Mpeg_DecodeMotionVector` 0x2ee2a8 (adds and wraps into ±16 << r_size, halves / doubles for field
  vectors), `Mpeg_GetDmVector` 0x2ed5a8 (VLC table 3), `Mpeg_CalcDualPrime` 0x2ed5c8 (the derived vectors for frame
  and field pictures, 0x35caf0 top_field_first).
- **Motion compensation and reconstruction:** `Mpeg_StartMotionComp` 0x2eb630 (non-intra: "Invalid modion type --
  ignored(%d)" outside 1..3, `Mpeg_FormPredictions`, then a SPR_TO DMA chain (tag ids 3 / 0, 0x30 qw luma and 0x30
  chroma per block) pulls the reference blocks into scratchpad; sets the slot's destination address in the target
  picture, macroblock size 0x180 bytes), `Mpeg_FormPredictions` 0x2eb968 (forward (mb_type & 8) and backward (& 4) by
  motion_type, frame or field, dual prime; "(a)/(b)/(c) invalid motion_type(%d)-0/1/2"), `Mpeg_QueuePrediction`
  0x2ec018 (one block: whole-pel position split into macroblock and offset, luma 16 and chroma 8 rows, the row split
  where a block crosses a macroblock, and the half-pel averaging routines from the tables 0x35d208 (luma) / 0x35d228
  (chroma) indexed by average << 2 | half-x << 1 | half-y), `Mpeg_ReconMacroblock` 0x2ec438 (runs the queued
  routines, then adds the IDCT result as halfwords, clamps 0..255 with MMI `pminh` / `pmaxh` (0x2ed380) and packs
  `ppacb` into the picture, 0x18 × 32 bytes; "intra && skip MB" if both flags are set), `Mpeg_WaitBdec` 0x2ed3c0
  (waits for IPU_FROM to drain, pumping the no-data callback; records IPU_TOP / bits left 0x35d1d8 / 0x35d1dc; on
  IPU_CTRL bit 14 "Error code detected(BDEC)", resets the IPU and stops IPU_FROM, returns false).
- **Output:** `Mpeg_OutputPicture` 0x2ef7c8 (after the first picture outputs the previous reference, or the B
  picture itself), `Mpeg_FlushLastPicture` 0x2ea9d0 / `Mpeg_OutputLastRef` 0x2eaa50 (at sequence end in field mode,
  outputs the last reference; "the second field is missing"), `Mpeg_OutputFrame` 0x2f00c0 / `Mpeg_OutputFieldPair`
  0x2f01d8 (`Mpeg_GetPictureInfo` 0x2eff28 for pts (interpolated from sys+0x78/+0x80/+0x88 when the stream has none),
  dts and a flags word (descriptor +0x2c type << 0, +0x30 structure << 3, +0x40 << 5, +0x3c top_field_first << 6,
  +0x38 << 7, +0x34 << 8) into sys+0x10/+0x18/+0x20; copies the display extension fields to sys+0xb4..+0xd0; `Mpeg_CheckOutputSize` 0x2efc60
  ("Too small buffer size for %dx%d picture\n"); sys+0xb0 picks the copy: 0 `Mpeg_CopyRawPicture` 0x2efd00 (the decoded
  YCbCr macroblocks as they are, through scratchpad by SPR_TO / SPR_FROM; that this is the RAW8 path is inferred), else `Mpeg_CscPicture` 0x2f0860 (IPU colour conversion to RGB32)).
  `Mpeg_CscPicture` feeds the picture to IPU_TO (in 0xffff-qw pieces by `Mpeg_IpuToDmaHandler` 0x2f0788, created,
  DMAC channel 4) and runs CSC 0x70000000: `Ipu_Csc` 0x2f0398 for up to 0x3ff macroblocks, `Ipu_CscLarge` 0x2f05c8
  for more (0x3ff at a time, `Ipu_CscDmaHandler` 0x2f0480, created, DMAC channel 3; "CSC handler error\n").
- **IPU DMA (Sony's libipu, names inferred from its documented API, the bodies match it):** `sceIpuStopDMA` 0x2f0b88
  (was a function in Ghidra but not in the index; stops IPU_TO, saves its MADR / TADR / QWC / CHCR, waits for the
  IPU output FIFO (IPU_CTRL bits 4-7) to empty, stops IPU_FROM, saves MADR / QWC / CHCR, IPU_BP and IPU_CTRL: the `sceIpuDmaEnv` layout),
  `sceIpuRestartDMA` 0x2f0c70 (puts them back, backing IPU_TO up by the input FIFO's unread quadwords from the saved IPU_BP, and re-issues
  the bit pointer as IPU command 0), `sceIpuSync`
  0x2f0dc0 (mode 0 waits while IPU_CTRL busy, 1 returns busy), `sceIpuInit` 0x2f0e90 (from `sceMpegInit`: reset,
  intra matrix 0x35d320 (4 qw) and non-intra matrix (the 0x35d360 quadword four times) by SETIQ, VQ CLUT 0x35d370 by SETVQ 0x60000000, SETTH
  0x90000000, reset). The CHCR writes with the DMAC suspended (D_ENABLEW bit 16): `Ipu_SetFromChcr` 0x2f0ab8,
  `Ipu_SetToChcr` 0x2f0b20, `Ipu_SetToChcr2` 0x2f0e28 (a second copy, in `sceIpuInit`'s file). The default
  StopDMA / RestartDMA callbacks of batch 33 jump to the first two.
- Inferred only: the names; that `Mpeg_DecodePicture` 0x2ea820 is the body of `sceMpegGetPicture` (no caller
  found). libmpeg has no unnamed functions left.

## Names from F1c batch 35 (proven unless marked)

- Streams: `Strm_SetPlaylistEntry` 0x2c7098 (entry n, 1-based up to header +0x1c, of the 12-byte table +0x18 gets
  (a3, a4, a5) while +0x34 is 0; a5 ≤ +0x24 or −1 / −2; "playlist" inferred), `Strm_GetWord4` 0x2c73a0,
  `Strm_FindBufferedRequest` 0x2c7d20 (walks the request list under the mutex with `RingBuf_InRange`),
  `Strm_GetRequestWord4` 0x2c7de8 (`Strm_LookupRequest` +4).
- `File_BatchLoad` 0x2c84a0 (n names → `File_LoadManyCb` under heap 0x3bb930), `FileSys_SyncOp0` 0x2c8ef8 /
  `SyncOp2` 0x2c9070 (queue, wait, result), `FileSys_QueueOp7` 0x2cac38 (op type 7, +0x10 / +0x14 = a1 / a2),
  `FileSys_NoOpHook` 0x2cb9c8, `FileSys_SetRootThunk` / `GetRootThunk` 0x2cc328 / 0x2cc348, `WStr_FormatThunk`
  0x2c5558, `EAMem_ReferThread` 0x2cf2f8.
- SDK: `Pad_RpcCmd14` 0x2e8288 (libpad RPC 0x403c40, command 0x14), `sceCdPOffCallback` 0x2e87c8 (stores the
  callback / argument at 0x404004 / 0x404008 between `DIntr` / `EIntr`, answers the old one: Sony's
  `sceCdPOffCallback(func, addr)`; named `Cd_SetPowerOffCallback` in batch 35, renamed after), `Cd_RunPowerOffCallback`
  0x2e8838, `Cd_NcmdRpc0E` 0x2e8e70 (N-command RPC 0xe, from `sceCdRead` / `sceCdStop`), `Mc_GetRpcBuffers` 0x2f1280,
  `Mc_RpcCmd15` 0x2f1368, `Thread_PostSelfEvent` 0x307830, `SifLoad_CheckVersion` 0x30d8b8 / `ResetVersion`
  0x30d948, `sceSifLoadStartModule` 0x30de10 (`SifLoad_ModuleCmd` with the result pointer a3 passed on, unlike
  `sceSifLoadModule`), `InitSys_ClearSyscalls` 0x30e470 (syscalls 0x80..0xff set to 0), `Libc_EioStubA..C`
  0x306df8 / 0x306f38 / 0x306f60 (errno 5, −1; A sits between `read` and `close`).

## Names from F1c batch 36 (proven unless marked)

- `Cd_InitPowerOff` 0x2e8878 (was `sceCdPOffCallback`; from `sceCdInit` and from `sceCdPOffCallback` 0x2e87c8 while
  0x35aebc < 0): `sceSifInitRpc`, 0x35aea4 = 1, SIF command handler 0x80000012 = `Cd_RunPowerOffCallback` 0x2e8838,
  then binds RPC 0x80000596 (client 0x404078; up to 18 tries, 0x35aebc = 0 once bound) and calls its function 1
  with 0xb at 0x4040a0; answers 1 on success. 0x35aea4 is 1 while it runs, which keeps the handler from calling the
  callback. The two names were swapped: 0x2e87c8 has the SDK function's signature and behaviour (both in
  `ghidra/renamed.txt`).

## The PS2 memory card driver, part 1 (row E12f1, proven)

`tricky_game::memcard`: `cPS2MemCard` (table 0x3a4598; base `cMemCard` 0x3a47f0) on the game's bytes, 80
functions checked in the runner (`tests/memcard.rs`, 12,000 cases; 24 of 25 mutants, the one left equivalent).
The starters only set fields and hand `MemCard_Process` 0x23e0d0 a state: 3 port set, 5 directory named, 6
directory read, 0x11, 0x12 formatted, 0x13 / 0x22 file picked, 0x14 directory opened, 0x15 save begun, 0x17 save
more, 0x18 save ended, 0x1b load begun, 0x1d load more, 0x1e load ended, 0x23 entry read, 0x36 save path; 1 after
any `sceMc*` error (the result kept in 0x345f54). Layout in the module's doc. The ports' results the game names:
-1, -3, -8..-10 and -10001/-10002 (vf8); `MemCard_GetSlotError` keeps ≤ 0 above -8 and -10001/-10002, else -10.
A save of n bytes takes `ceil(n/1024) + 0x83` blocks (vf18). Five starters and helpers have no function in the
index (0x23dc50 entry read, 0x23dd18 `sceMcFormat`, 0x23dfc0 load begin, 0x23ebf0 port changed, 0x23ec08 the
port's line, 0x23ecf0 a file's name copied); ported and checked with the rest. The rest of the driver followed in
rows E12f2a and E12f2b (below).

`MemCard_Process` 0x23e0d0 (row E12f2a, proven; `memcard::process_state`, second test in `tests/memcard.rs`: every
state 0..0x3f and random ones, 19 of 21 mutants, 2 equivalent on the ELF's data): jump table 0x3a42c0. Every
open and close leaves the handle (+0x18) at -1 whatever the result (the handle comes later, from the sync in vf2);
an error goes to state 1, except while reading the directory (8, 9, 0xb..0xf go to 0x10, a failed read 0xa to
0xf, a failed open 8 or read marks the file, +0x1a0). 0x1c and 0x1d treat any nonzero answer as an error. State
0x26 builds the icon.sys block at 0x346010 ("PS2D", the title in SJIS from +0x50, its length +0xc or 2·strlen,
0x60, the colours and lights from 0x345f60..0x346010, the three icon names) and writes its 0x3c4 bytes.

The SJIS converters (row E12f2b, proven; third test in `tests/memcard.rs`, 5 of 5 mutants): `Ascii_ToSjis`
0x23d770 maps space and punctuation (0x20..0x2f, 0x3a..0x40, 0x5b..0x60, 0x7b..0x7e) through the table 0x345e98
(index `c − (k + 0x1f)` with k 1, 0xb, 0x25, 0x3f) and digits / upper / lower case by the group starts at 0x345e88,
answering the code byte-swapped (as stored) and sign-extended; `Sjis_ToAscii` 0x23d868 reverses the 0x81 row
(0x345ee8 − 0x40) and the 0x82 row's three groups (0x345ee0). `MemCard_AsciiToSjis` / `MemCard_SjisToAscii`
(0x23d628 / 0x23d680) run them over a string.
`MemCard_CheckRoomForFiles` 0x23fcb8 (E12f2b, proven, `memcard::check_room`, in the first test): free blocks (vf +0x88) less a save's (vf +0x98 of +0x24) for each file the card lacks (vf +0x1c8 answers 0); a file that does not fit is marked +0x1a4 (signed compare).
`cPS2MemCard::vf13` 0x23fb60 (E12f2b, proven, `memcard::file_line`): a file's line for the menu: busy text 0x3a4590 in states 5..0x10, else the strings 0x15f (no card, or the file not on it), 0x1cf (card unusable), 0x148 (file flagged), 0x18b (no room), else the file's title (+0x1e8); cut to 32 characters on the stack and copied out.

`cPS2MemCard::vf2` 0x23edc8 (E12f2b, proven, `memcard::update`, the first test with 8000 cases of its own over every
state 0..0x3d, 16 of 16 mutants): the driver's per-frame step; first and second jump tables 0x3a43b0 (state − 2)
and 0x3a44a0. In order: an unformatted current port (+0x180 clear) forgets its files (+0x4c 0, +0x19c..+0x35c
cleared). A pending `sceMcGetInfo` (+0x3c) is waited on with `sceMcSync(1, 0, 0x345f54)` (answer 0: still busy,
return); -1 sets the port's changed flag (+0x184); results in -19..-11, -49..-40, -59..-50, -79..-70 become -10;
a result `MemCard_IsErrorCode` does not name becomes -10002 when the card type (0x345f4c) is not 2, else -2 when
unformatted (0x345f58 clear); then the port keeps result, type, free blocks and formatted (+0x17c, +0x174, +0x178,
+0x180) and +0x3c clears. The states that started a card call (2, 6, 8..0xf, 0x12, 0x16..0x18, 0x1c..0x1e,
0x23..0x27, 0x29..0x2b, 0x2d..0x2f, 0x31..0x33, 0x36..0x3b) sync the same way; on a negative result: -4 is no
error for 0x24 (make directory) and 0x36..0x3b (deletes); in 8..0xf an error the game does not name (and not -1)
flags the file being read (+0x19c) and goes to 0x10; any error left goes to state 1 (0x23 / 7 clear the entry, 6
resets the driver, 0x13 forgets the picked file); else the port's result clears. Then +0x40 (load ended) goes to
0x1e and +0x44 (save ended) to 0x18. The steps: 5, 0x13, 0x14, 0x15, 0x1b, 0x22 wait for vf +0x1e8 to answer 0,
then read the directory / build the save path / 0x24 / 0x16 / 0x1c / read the entry (0x23dc50); 6 counts the
entries (+0x4c = min(read, files)) and gives file i the first directory name (entry +0x20, from +0x3e0) ending in
the digit i + 1; 7 and the end of the file walk check the room; 8 keeps the handle and reads (0xa), 0xa converts
the save's SJIS title (0x3460d0, the icon block's) into the file's title (+0x1e8); 0xb flags the file when the
entry's word (+0x390) is not the save's size (+0x24); 0xc..0xe mark it (+0x1a0) when the entry's word is not the
icon's (0x3a49e0: 0x1f788, 0, 0), else, as do 0xf and 0x10, go on to the next file the card has (vf +0x1c8) with
its `icon.sys` path (0x3a49f0); 0x11 formats an unformatted card (0x23dd18); 0x16, 0x1c, 0x25, 0x29, 0x2d, 0x31
keep the handle and go on; 0x19 gives the picked file the name (+0xb1) and title (+0x50); 0x1d checks the bytes
read against +0x20 (else -10001, state 1); 0x23 clears the entry unless the result is 1; 0x28 / 0x2c / 0x30 wait
for the icon's load (`AsyncFile_IsDone` 0x2c6108 answering 1, then `AsyncFile_Finish` 0x2c6160 into +0x30 / +0x34);
0x34, 0x35 stay. A step that falls through ends: `MemCard_GetSlotError` -10 or -1 clears the port's status, -10 or
-2 resets the driver, and a state other than 0 goes back to 0.
`MemCard_FormatSaveTitle` 0x1ce8f8(o, n) (E12f2b, proven, `memcard::format_save_title`, fourth test, 8 of 8
mutants): a front-end object's save title: `o`+0 clear: the string 0x1cc formatted (`WStr_Format`) with n + 1 and
the course's code (the first 4 letters of the course table's +0x10, 0x332f28 + 0x1c · course 0x32de88), else the
string 0x1cb; handed to the card (`o`+0x224, its vf at +0x130, vf37: into its +0x50 as ASCII) and copied to
`o`+0xd8. The library's `strncpy` (0x2fd630) is done in the module.

## The course's lookups (row E3f; `game/src/course_lookup.rs`, `tests/course_lookup.rs`, proven)

Checked in the runner (18,000 cases on random courses, 12 of 12 mutants). The world 0x346488 keeps the resolved
course at +4.
- Records by number, answering 0 when the number is not below the count (signed):
  - `Course_GetParticleInstance` 0x254a50: 0x70 each, count +0x10, table +0x4c;
  - `Course_GetLight` 0x254a80: 0x5c each, count +0x1c, table +0x58;
  - `Course_GetSplineSegment` 0x254b20: 0x28 each, count +0x20, table +0x5c.
- `Course_GetHashTables` 0x254ba8: the hash block at course +0x80. Each kind's count and table are in that block:
  patches +4/+8, instances +0xc/+0x10, models +0x14/+0x18, lights +0x1c/+0x20, splines +0x24/+0x28, camera
  animations +0x2c/+0x30. The twelve getters 0x254bb8..0x254e20 answer 0 without a block.
- `Course_HashBsearch` 0x254ed8(lo, hi, key, table): entries of 8 bytes (hash, record), unsigned compares, the
  middle by an arithmetic shift; -1 when the key is absent.
- `Course_Find*ByHash` (0x254f30 lights, 0x254fb8 patches, 0x255040 models, 0x2550c8 instances, 0x255150 camera
  animations, 0x2551d8 splines): a search over 0..count−1, the entry's record or 0.
- `Course_FindObjectByHash` 0x255260(w, hash, kind): only with a spline table. It tries patches (kind 1), instances
  (2), lights (4), splines (5) and camera animations (6), in that order, never models; else kind −1 and 0.
- `CourseResolve_IndexTable08` / `0C` (0x254e58 / 0x254e98): fill the resolver's +8 / +0xc with the address of each
  record counted at course +0x18 / +0x28, starting at +0x54 / +0x64. Each record is a word count, then that many
  words. They answer 1.
- `Course_DebugPrint` 0x255310 only spills its arguments.

## The course resolver's steps (row E3g; `game/src/course_resolve.rs`, `tests/course_resolve.rs`, proven)

Checked in the runner (8,400 cases, 15 of 16 mutants; the other, the material block's count read again after each
material, is equivalent because nothing in the block can change it). The resolver `cCourseResolve_Common` (table
0x3a7058 at +0x10) keeps the course at +4 and the instances' and particle instances' index tables at +8 / +0xc.
- Constructor and destructor: the table, then the virtual at +0x18 (`Reset` 0x2522a0: +0xc, +0, +4, +8 cleared)
  through vf1 / vf3. The destructor deletes on bit 0.
- `ResolveDataOnly` 0x252408(r, course): 4 without a course, 6 with more than 4000 particle instances (course
  +0x2c, signed). Else the index tables go on its own stack frame (0x4e30 bytes: +8 at the frame, +0xc 16,000 bytes
  up), the course is kept, and the virtual +0x58 runs on it. The answer is 1 when that virtual answers 0, else 0.
- Each step prints its name with `Course_DebugPrint` (strings 0x3a6b70.., which print nothing) and answers 1:
  - `ResolveInstances` 0x253400, 0x100 bytes each: +0xc0 by id (0x2522e0), +0xf4 cleared, +0xf0 from the index
    table, +0xc4 / +0xc8 instances by number (0x254a28), 0 when negative.
  - `ResolveParticleInstances` 0x2534c8, 0x70 each: +0x40 by id, +0x64 cleared.
  - Model and particle model pointers (0x253540 / 0x253b60) add the base to `n` words.
  - Models and particle models (0x2535b0 / 0x253bd0): size-prefixed records handed to the virtual +0x78 / +0xc8.
  - Particle model hierarchies 0x253d30, 0x10 each: the three words printed; +4 / +8 made pointers when set and
    handed to the virtuals +0xd8 / +0xa0; +0xc the hierarchy, or 0x346440 for -1.
  - Particle mesh 0x253e80: its fields printed, the two float triples through `__extendsfdf2`; +0x24 made a
    pointer.
  - Material blocks 0x253fb0: a count, then that many materials by number (0x254af0).
  - Splines 0x254160: each segment's +0x20 becomes a spline. Spline segments 0x2541e8: each spline's +0x50 / +0x54
    become splines (0 for -1) and +0x58 a segment.
  - Materials 0x254600: 0x48 each, handed to the virtual +0x110.
  - `ResolveMeshContext` 0x253b58 only answers 1. vf33 0x254678: `x`+0x44 becomes a particle instance from the +0xc
    table (0 for -1).

## The patch and collision caches (row E3h; `game/src/patch_cache.rs`, `tests/patch_cache.rs`, proven)

- A cache (`cBxCache`, `cAIFwdDiffCache` derives from it with only its own table 0x3a6508): +0 `n` records, +4
  their size, +8 `n` 8-byte entries, +0xc the records, +0x10 the table (0x3a6518). The world's is at `cWorld`+0x28
  (built by `cCourseResolve`'s constructor 0x25ece8, sized by `CollCache_Init` from `Course_LoadRenderData`,
  freed by `Course_Unload` and `World_Destruct`); `cAIWorld::vf1` ages the one at 0x3464b0 each tick.
- `CacheBuffer_Init` 0x251c20 frees first, then allocates records (name 0x3a64e8) and entries (0x3a64f8) with
  `EAMem_Alloc`, fills each it got with 0xdeadc0ed (`EAMem_Set`) and zeroes the entries; `CollCache_Init` uses
  0x1cd0-byte records and clears each record's key (+0).
- An entry is (hits this tick, last tick's hits − 1): `CacheBuffer_Touch` adds a hit (capped below 2000000000),
  `PatchCache_Age` moves hits − 1 to the second word and clears the first.
- `CacheBuffer_Evict` takes the first entry whose two words sum to ≤ 0, otherwise the smallest sum (first of
  equals; when no sum is under 2000000000, entry 0), sets its hits to 2 and answers its record. With `n` ≤ 0 it
  still writes entry 0.
- `CacheBuffer_Lookup` (from `World_LookupCollCache` 0x25c750) finds the record whose key is the patch (its
  address) and touches it; on a miss it evicts one, stores the key, runs the VU0 patch program on the patch's
  coefficients (key+0x50), clears the record (`CacheBuffer_ClearEntry`: words +0, +0x10, +0x20 of 81 0x40-byte
  blocks from +0x650 and 9 from +0x1a90 set to 1e10) and copies VU0's output to +0x10.
- Checked in the runner (13 functions, 13 of 14 mutants killed, one equivalent; `Mem_Free`, `Mem_Delete`, `EAMem_*` and the VU0 calls stood in).

## The course models' DMA resolve and the world's resolver (row E3i; `game/src/course_dma.rs`, `tests/course_dma.rs`, proven)

- `CourseDma_TryResolveModels` 0x255df0 (from `Course_LoadRenderData` and `Course_LoadSky`): 4 without a course,
  6 with more than 4000 models (+0x2c), else the models (+0x6c, each as long as its first word) → hierarchies
  (model +4 count, +8 first, 0x18 each; model +0x10 flags) → up to three meshes each (+4, +8, +0xc; one equal to
  an earlier one of the three is skipped, but nothing stops a mesh being met again in another hierarchy) →
  contexts (mesh +0x20 count, +0x28 list). Each step prints a "### RESOLVING ..." line (`Course_DebugPrint`, a
  no-op).
- A context: +0xc its VIF DMA chain, +0x14 / +0x18 / +0x10 three chain offsets (slots). Flag 0x10 → the chain is
  rebuilt (`CourseDma_RebuildChain` 0x240c78), else patched (`CourseDma_PatchContext` 0x240a98); then +0x10 =
  chain + (+0x10).
- Patching, in place: CNT data copied down; a REF to the first / second slot → a quadword `0, MSCAL 0x26` /
  `0, MSCAL 0x3c` (VIF code 0x14 in the high word); a REF to the third → `0x3000000005000001, 0`; the END copied
  with its data; the first tag's count = quadwords written, its high word cleared.
- Rebuilding, in place, into runs: the first tag is the open run; CNT data and the MSCAL quadwords are counted into
  it; a REF to the third slot closes the run (`count | 0x10000000`, i.e. a CNT), is written back with its address
  made absolute (chain + offset), and the tag after it becomes the next run's tag; the END closes a non-empty run
  and is copied with its data.
- Both loop for ever on any other tag (an id other than CNT, REF, END, or a REF to none of the slots): our port
  panics instead (STANDIN). Both copy with `EAMem_Copy` 0x2ce114 between overlapping places (the destination never
  after the source in patching; in rebuilding a REF to the third slot puts it 0x10 ahead, onto the next tag only
  after it has been read).
- `cCourseResolve::cCourseResolve` 0x25ece8 (the world at 0x346488, from `StaticInit_World`): the PS2 resolver's
  constructor, table 0x388350, the patch cache (+0x28, row E3h), the visible list (+0x3c), +0x14 and +0xac8 = 0,
  0x200 words from +0x2c8 = −1, then the table's +0x14 (`cCourseResolve_Common::vf1`). `World_Destruct` 0x25ed70:
  the table's +0x24, `CellList_Destruct`(+0x3c, 2), the patch cache's destructor, `~cCourseResolve` 0x186168.
- Checked in the runner (12 functions, 15 of 15 mutants; chains of random CNT / REF / END tags with random junk bits; models whose
  contexts' chains are a lone END, which both rewrites leave as they were).

## The resolver's header pass and the collision free (row E3j; `game/src/course_resolve.rs`, `tests/course_header.rs`, proven)

- `cCourseResolve_Common::ResolveDataHeader` 0x252e18(r, h) (no direct caller: a virtual of the resolver): prints
  the header (`Course_DebugPrint`, a no-op): its three version bytes (signed), the kind byte +3 (0 to 3, each its
  own text; any other kind answers 0 with nothing resolved), then the 32 words +4..+0x80 with their names. Then
  each offset that is not 0 becomes a pointer (`h` + offset) and goes with its count to one of the resolver's
  virtuals (table +0x10, adjust at `off`, function at `off + 4`):
  - +0x50 (no call yet); +0x54 → +0x98(count +0x18, p); +0x64; then `CourseResolve_IndexTable08` 0x254e58 and
    `0C` 0x254e98 (E3f; they read the course at r+4, so they see +0x54 / +0x64 already resolved);
  - +0x50 again (already a pointer) → +0x108(count +0x14, p);
  - pairs whose second offset decides, the first made a pointer with it untested: +0x68/+0x6c → +0x68(count
    +0x2c, a, b) then +0x70(count, b); +0x70/+0x74 → +0xb8, +0xc0 (count +0x30); +0x78/+0x7c → +0xf0, +0xf8
    (count +0x38); the counts and pointers are read again for the second call;
  - +0x40, +0x44 (no call); +0x48 → +0x60(count +0xc, p); +0x4c → +0xb0(count +0x10, p); +0x58 (no call);
  - +0x5c decides, +0x60 made a pointer with it → +0xe0(count +0x20, a, b), +0xe8(count +0x24, a, b);
  - +0x80 → +0x118(p). Then a closing print; answers 1.
- `Course_FreeCollision` 0x25fd40(c) (from `Course_Unload` 0x25f498): `Mem_Free` 0x14a040 on +0x14 and +0x20 (each
  cleared after); the array at +0x18 (8 bytes each, gcc's `new[]` count 0x10 bytes before it) walked from the end
  through `Mem_DeleteIfFlag9` 0x260fa0 with 0 (which deletes nothing; our port leaves the call out), then freed
  from its count and cleared; +0x24 freed; the array at +0x1c (0x150 bytes each) destroyed from the end through
  each element's virtual +8 (with 0), freed and cleared. Both loops read the array's pointer again on every step.
  Last, a `SphereTreeBufCache` (table 0x3a40f8) is made on its own stack (sp − 0x50), emptied
  (`SphereTreeBufCache_FreeAll` 0x238dd0) and destroyed (0x23b520 with 2).
- Checked in the runner (4,000 cases, half each; random headers with real record lists for the index tables, random
  arrays with counts 0 to 3 and elements on two tables with adjusts 0, 0x10, −8); 19 of 19 mutants (two of them, wrong strides in the free, loop for ever and were
  killed by a time limit).

## The async BIG file class (row E2d; `game/src/asyncbig.rs`, `tests/asyncbig.rs`, proven)

- `cAsyncBigFile` (table 0x3a41a8; files at `cGame`+0xfc / +0x110, `AnimData`+0x710, and the base of
  `cMemoryBigFile`) reads files by name through EA's async file calls (`AsyncFile_*` 0x2c5e20..0x2c6278). All files
  share one table of 100 requests of 0x5c bytes at 0x343a58 (cleared by `BigFile_ClearTable` 0x23be28) and one
  lock (`RecMutex` at 0x343a50, made by the first constructor, destroyed by the last destructor, the count at
  0x343a54). The request's fields are in the module's doc comment; its state is 0 free, 1 waiting, 2 reading,
  3 read.
- The virtuals as read (their slots in the table: vf6 +0x38, vf7 +0x40, vf8 +0x48, vf9 +0x50):
  - vf15 0x23c210(f, name, w, cb, arg), the request: the name made by vf7 (`|` put in front unless there is one)
    into a 128-byte buffer on its stack; a request in any state but free with the same name gets one more holder
    (+0x58) and its number is the answer. Else the first free one is filled (state 1, one holder) and the answer
    is its number, or −1 with none free. Both of those run vf6.
  - vf6 0x23c088(f), start: nothing when the file reads one at a time (+0xc) and one runs (+8), or when +0 is 0.
    Each of the file's waiting requests becomes reading and gets its handle from vf8 (`AsyncFile_Load`(name, +0x10)
    with `AsyncBigFile_FinishRead` 0x23beb0 as callback). It stops when that read is already over (state 3) or,
    one at a time, after the first (+8 = 1).
  - `AsyncBigFile_FinishRead`: under the lock, `AsyncFile_Finish` fills two words on its stack (the buffer and a
    second word); `AsyncFile_CompleteRequest` 0x23bf18 marks every reading request with that handle read, keeps the
    buffer at +0x1c and the word at +0x14, runs its callback (+4) with (+8, buffer), clears the file's +8 and runs
    vf6 again when the file reads one at a time.
  - vf11 0x23c370(f, i): read or not. vf14 0x23c1d0: how many requests are waiting or reading (all files).
  - vf12 0x23c398(f, i, flags): the data, made the first time (vf13 0x23c5c0(f, i, flags, name) makes it every
    time and hands it over, +0x20 cleared). The buffer is unpacked (`Compress_DecodeApi` 0x2c2f90) when
    `Compress_UnpackedSizeApi` 0x2c3190 gives a size, else copied whole. The memory comes from the pool 0x345e48
    (`MemPool_Alloc`) with flag 0x40000000, else `EAMem_Alloc`(name, size, flags & 0x3ffffff). When it comes, it
    is filled with 0 (flags negative) or 0xdeadc0ed. The buffer is then freed. A failed allocation is still
    unpacked or copied into (address 0).
  - vf10 0x23c8d0(f, i), release: one holder fewer; at none (or fewer), under the lock: vf9 (cancel and finish the
    read) when reading, the buffer and data freed, the request zeroed, the file's +8 cleared.
  - vf1, vf4 empty; vf5 answers +0. The destructor runs vf2 (empty) when +0 is set.
- `BigArchive_Reset` 0x23bbe8 (the other BIG reader's state, `BigArchive_*`): +0x88 = −1, 0x81 bytes from +4
  cleared, +0 = 0. `BigArchive_Init` 0x23bb70 resets.
- Checked in the runner (19 functions; 6,300 cases on a random request table, the name maker, the read starter
  and `AsyncFile_Finish` stood in on both sides); 37 of 38 mutants. The survivor is equivalent: the constructor reads the
  file count again after `RecMutex_Create`, which does not change it.

## The in-memory BIG file class and the BIG wrappers (row E2e; `game/src/memorybig.rs`, `tests/memorybig.rs`, proven)

- `cMemoryBigFile` (table 0x3a4120; two made by `vf1_263b30` 0x263b30) is a `cAsyncBigFile` (row E2d) whose
  archive is loaded whole: vf1 0x23ca70(f, name, w) puts `File_LoadAlloc` 0x2c8018(name, w) at +0x14, vf2 0x23caa0
  frees it (`EAMem_Free`). It uses the same request table (vf15, vf14 are the base class's), but:
  - vf6 (start reads) does nothing and vf11 (read?) answers 1;
  - vf7 drops a leading `|` from the name (the base class adds one);
  - vf12 0x23cb80(f, i, flags): the first time, the entry is found by the request's name (`BIG_Find` into two
    words on the stack, 0 first: offset, size), +0x1c = archive + offset, then unpacked (`Compress_DecodeApi`)
    when `Compress_UnpackedSizeApi` gives a size, else copied (the entry's size), into memory got as in the base
    class (pool on 0x40000000, else `EAMem_Alloc` named 0x3a4110, filled with 0 or 0xdeadc0ed when it comes). +0x1c
    is cleared (nothing freed: it points into the archive) and the memory kept at +0x20;
  - vf13 0x23cdb8(f, i, flags, name): 0 when +0x20 is set; else the same, handed over (+0x20 left 0). The `name`
    argument is not used: the memory is always named 0x3a4110;
  - vf10 0x23cad8(f, i): one holder fewer; at none, the data freed (`Mem_Free`) and cleared, the request zeroed
    (no lock, no read to stop).
  - The destructor clears +0, frees the archive, then runs the base class's destructor.
- The BIG wrappers (over `BIG_Lookup` 0x2cbdc0(big, name, index, &offset, &size), which answers the entry's name or
  0): `BIG_Find` 0x2cc010 passes its five arguments on. `BIG_GetFileOffset` 0x2cc030(big, name): the offset (0 first,
  no lookup without a name). `BIG_GetFileData` 0x2cc0a8: big + offset, or 0 for offset 0. `BIG_GetFileDataByIndex`
  0x2cc060: big + offset when found, else 0. Both offset lookups pass gp − 0x7f90 (0x3bb960) for the size, a word
  nobody reads. `BIG_GetFileDataList` 0x2cc120(big, "a,b,c", out): each name's data into the words from `out`,
  each comma made 0 for the lookup and put back. `BIG_GetFileName` 0x2cc268(big, index, out): the name copied,
  `out` answered; else `out` emptied and 0.
- Checked in the runner (16 functions, 4,800 cases; `BIG_Lookup` stood in on both sides: it answers a name or 0 and
  fills the offset and size words it is given when it finds the entry; the stack starts
  with junk so the cleared words show); 29 of 29 mutants.

## The async rider loader (row E2f; `game/src/bdrload.rs`, `tests/bdrload_async.rs`, proven)

- `cAsyncBdrLoad` (vtable 0x39de40 at +4; its base `cBdrLoad`, vtable 0x39e280): +0 a mode (`AsyncBdrLoad_SetMode`
  0x201788 writes it), +8 / +0xc / +0x10 the managers (`AsyncBdrLoad_SetManagers` 0x201c10: rider meshes, board
  mesh, textures; the board's may be 0, then the rider meshes' does it), +0x14 twelve slots of 0x50 bytes.
  `AsyncBdrLoad_Init` 0x201b50 clears the mode and every slot's state.
- A slot (the record `BdrLoad_BindTextures` 0x2021f8 and `BdrLoad_BuildSkin` 0x201138 take): +0 state (0 free,
  1 loading, 2 loaded), +4 the rider, +8 an owner, +0xc the request word, +0x10..+0x18 the body, head, board mesh
  ids, +0x1c..+0x24 their data, +0x28..+0x38 the head, helm, suit, boot, board texture ids, +0x3c..+0x4c their data.
- The request word: byte 0 a kind (1 from the front end's requests, 0xf from `BdrLoad_DefaultRequest` 0x201a98),
  bit 8 rider meshes, 9 board mesh, 10 board texture, 11 suit and boot, 12 head and helm, 13 (set by
  `BdrLoad_FEBoardRequest` 0x201ad8, cleared by `BdrLoad_FERequest` 0x201a38 and the default; meaning not read).
- The names come from the exe's tables by the rider's choices (its +0x464 record): body .mpf 0x3419c8[+0x50], head
  .mpf 0x341a00[+0x58], the one board .mpf 0x341f98, head/helm .ssh pairs 0x341ce8[+0x54], suit/boot .ssh pairs
  0x341a38[+0x4c], board .ssh 0x341d50[+0x48].
- `AsyncBdrLoad_Request` 0x201e68(load, rider, owner, word): the first free slot (−1 when all twelve are taken),
  loading, ids −1, then each asked-for file started through its manager's virtual +0x80 (name, 0x10, 0, 0), whose
  answer is the id. The word goes through the stack and `EAMem_Copy` into +0xc, then is stored there again.
- `AsyncBdrLoad_IsSlotReady` 0x201d30: 1 for a slot not loading; else each id ≥ 0 asked of its manager's +0x60,
  0 at the first not ready, else the slot becomes 2 and 1. `AsyncBdrLoad_FreeSlot` 0x201c20: each id ≥ 0 let go
  of through its manager's +0x58, the slot freed (whatever its state). The board mesh id is read again at +0x18
  when the board manager does it.
- `BdrLoad_RequestRider` 0x201790: the same at once, on a slot on its stack: the files loaded whole (`File_Load`,
  0x10) into the data words, state 2, `BdrLoad_BuildSkin`, then every file freed (`EAMem_Free`).
- `cAsyncBdrLoad::~cAsyncBdrLoad` 0x201b90 frees the slots in use, then `cBdrLoad::~cBdrLoad` 0x202478 (its
  vtable, `Mem_Delete` with flag 1).
- Checked in the runner (12 functions, 4,800 cases; the managers' virtuals, `File_Load`, `EAMem_Free`,
  `EAMem_Copy`, `BdrLoad_BuildSkin` and `Mem_Delete` stood in on both sides: the copy copies only sometimes, so the
  second store into +0xc shows; the skin build logs the slot it is handed and sometimes clears one of its files);
  39 of 40 mutants. The survivor is equivalent: `BdrLoad_RequestRider` reads the word back from its stack for +0xc,
  where nothing else wrote.

## The BIG archive's open, close and delete, the request table's clear, the read buffers (row E2g; `game/src/asyncbig.rs`, `tests/bigarchive.rs`, proven)

- An archive (`BigArchive_Init` 0x23bb70, row E2d): +0 open, +4 its name (0x81 bytes), +0x88 the mount's handle.
  `BigArchive_Open` 0x23bc28(a, name, flags): the name copied to +4 (`strcpy`); for the while, the word 0x3bb978
  holds `flags` (the old value put back after; the word is read by the allocations, inferred); `FileSys_OpenSync`
  0x2c9018(+4, 100), and only when that answers non-zero `FileSys_MountBig` 0x2c8f50(+4, flags, 100, +0x88); +0 is
  1 when both went (written after each), and is the answer.
- `BigArchive_Close` 0x23bcc0(a): when +0 is set, `FileSys_UnmountBig` 0x2c8fc0(+0x88's handle, 100); then
  `BigArchive_Reset`; answers 1 when it unmounted (0 when it was not open). `BigArchive_Delete` 0x23bb98(a, flags):
  closed, then `Mem_Delete` with flag 1.
- `BigFile_ClearTable` 0x23be28: `memset` of the 100 requests (0x343a58, 0x23f0 bytes). `cAsyncBigFile` vf2
  0x23bea0 and vf3 0x23be98 do nothing.
- `File_GetUnpackedSize` 0x23c7c8(f, i): request `i`'s read buffer (+0x1c): `Compress_UnpackedSizeApi`, else (not
  packed) `EAMem_GetSize`. `File_DecompressBuffer` 0x23c818(f, i, dst): the same choice, then
  `Compress_DecodeApi`(buffer, dst) or `EAMem_Copy`(dst, buffer, size); the buffer freed (`EAMem_Free`), +0x1c and
  +0x20 cleared. The file argument is not used in either.
- Checked in the runner (8 functions, 4,000 cases; the file system, memory and compression calls stood in on both
  sides; the opens and mounts log the flags word they see); 23 of 23 mutants.

## The BIG library checked (row E2h; `data/src/big.rs`, `game/tests/big_lib.rs`, proven)

- `BIG_Identify` 0x2cbc88, `BIG_HeaderSize` 0x2cbd08 and `BIG_Lookup` 0x2cbdc0 (row E2a) and two more now ported:
  `BIG_GetFileCount` 0x2cc1b0 (`C0FB`: the big-endian u16 at +4; `BIGF`: the u32 at +8; else 0) and
  `BIG_FindEntry` 0x2cc0d8(big, name) (0 without a name; else `BIG_Lookup` by name with the offset word on the
  stack, 0 first, and the size into the word 0x3bb960; big + offset, or 0 for offset 0).
- `BIG_Lookup` answers the entry name's address (now `BigEntry::at`) and writes the offset and size into the words
  it is handed (each may be 0: not written), both 0 when it finds nothing. Names compare with `stricmp` 0x2fcc98
  (the C library's: case through the ctype table 0x3ba521, which for ASCII is `to_ascii_lowercase`).
- Not followed by the port (data the disc does not have): the walk compares addresses, so a directory size that
  wraps past the end of memory ends it at once in the game; and the game reads names past the directory's end
  where our slice stops.
- Checked in the runner: 3,000 random directories (`C0FB`, `BIGF`, neither; 0 to 20 entries; sizes ending early,
  mid-entry or late), each with 6 lookups by number and by name (other case, prefixes, absent), with and without
  the out words; the original's `stricmp` and `strlen` run as they are; 18 of 18 mutants (one, `stricmp`
  without its end test, by hanging).

## The memory library's fill, copy, move and checksum (row E2i; `game/src/eamem.rs`, `career::crc16`, `tests/eamem.rs`, proven)

- `EAMem_Set` 0x2cc808(p, v, n): a fill from the word `v` (not a byte): a byte (v's low byte) to reach a 2-byte
  boundary, a half (v's low half) to reach 4, a word to reach 8, then 64 bytes a step (`v` twice per doubleword),
  16, words, a half, a byte; each step only with enough left. So a fill starting at an odd address puts v's low
  byte there and its low half next, not the word's byte pattern. `n` ≤ 0 fills nothing.
- `EAMem_Copy` 0x2ce114(d, s, n): forward. Under 18 bytes byte by byte; else bytes until `d` is on 16 (the loop
  would stop with the count at 0, the byte read and not written: unreachable from 18), then with `s` on 16 too
  128 bytes a step as two `lq`×4 then `sq`×4 halves, 8 a step (`ld`/`sd`), then bytes; with `s` off 16, bytes.
  An overlapping copy forward (d a little past s) therefore repeats the source in 64- or 8-byte runs, as on the PS2.
- `Mem_Move` 0x2ce228(d, s, n): backward byte by byte when s < d < s + n (unsigned), else `EAMem_Copy`.
- `Crc_Compute16` 0x2ce04c(p, n) is `career::crc16` (CRC-16/ARC tables at 0x34f0f8 / 0x34f1f8, from 0xeafb).
- Checked in the runner: 20,000 cases, every alignment, counts 0 to 0x300 and negative, sources overlapping the
  destination either way (729 backward moves), checksums of 0 to 0x80 bytes. 30 mutants: 17 killed, 13
  equivalent: they change only which store size writes the same bytes (the fill's 64- and 16-byte steps, its
  alignment steps at the edge counts, the copy's byte path for counts under 128, where a 16-aligned source and
  destination can only overlap 16 or more apart, its 8-byte steps split in two), an unreachable return, the move's
  edge cases d = s and d = s + n (both copies give the same bytes), and the checksum's 4-byte step at exactly 4.

## The memory library's heaps (row E2j; `game/src/eamem.rs`, `tests/eamem_heap.rs`, proven)

- A heap is a run of blocks with 16-byte headers: +0 a tag (0x424d "MB" when made, 0x4246 "FB" free, 0x424f
  "OB" taken from the free list, 0x4253 "SB" the two ends), +2 flags (low 4 bits the heap's number, 0x4000 free,
  0x8000 an end block, 0x100 carries its name, 0x200 "BEND" after the data, 0x800 four zero bytes 12 past the
  data, plus the 0x10 / 0x20 / 0x40 of the allocation), +4 the data's size (a free block: its whole span, header
  included), +8 the next block, +0xc the one before. Free blocks add +0x10 / +0x14, the next and previous free
  block, in a doubly linked list kept in **address order**, through a sentinel. A new free block is put in by
  walking from whichever end of the list is nearer (the midpoint of the first and last free block's addresses).
- The heap's record is the data of its first block (base + 0x10, 0x44 bytes): +0 the name, +8 the base, +0xc
  the top block, +0x10 the sentinel (tag 0x4253, size 0x7fffffff, +0x20 / +0x24 first and last free block),
  +0x28 alignment, +0x2c large alignment, +0x30 bytes kept after every block's data, +0x34 flags, +0x38 the lock,
  +0x3c a word from the caller. The 16 heaps' records are at 0x34eff8, by number. `EAMem_InitMainHeap` 0x2cd798
  makes heap 0 ("RAM", alignment 0x40, large 0x80, nothing kept, a lock, +0x3c = 0x2cd790) and copies it to slot 1.
- `EAMem_InitHeap` 0x2cd278(id, name, base, size, align, big, after, bend; on the stack: unused, names, lock,
  word): the first free block starts at the first `big`-aligned address past the record and `after` (+0x54 +
  `big` − 1, rounded down, − 0x10); the top block sits `after` + 0x30 short of the end. The two end blocks are
  named "%s LOW" / "%s HIGH" (0x3bb998 / 0x3bb9a0) when the heap names its blocks. Returns the free block's size.
- `EAMem_AllocEx` 0x2cca00(name, size, align, offset, flags): first fit from the list's start; from its end with
  0x10 (the block then taken from the free block's end); the **largest** block with 0x20 (the first of equal
  ones). `align` 0 means the heap's alignment, or its large one with 0x40; the data is placed so data + offset is
  aligned. Needs size + kept bytes (+ the name and its 0 in a named heap), at least 8, + alignment − 1 + 0x10.
  What is left becomes a free block (before the new one with 0x10, else after it at the heap's alignment) only
  when more than alignment + 0x2f; else the padding stays in the block. Nothing fits: the function at 0x3bb990
  (gp − 0x7f60) is called with (name, size, flags) when set, and the search runs again while it answers 1.
  Negative sizes give 0. `EAMem_Alloc` 0x2ccf70 / `EAMem_Alloc0` 0x2ccf98 / `EAMem_AllocAligned` 0x2ccf50 call
  it (the word they pass in t1, 1 or 0, is never read).
- `EAMem_Free` 0x2ccfc0(p): joins the free block before and the one after (each taken out of the list first),
  then puts the result in the list; 0 does nothing; returns 1.
- `EAMem_Resize` 0x2cda18(p, size): joins a free block after it, then the new size rounded with the kept bytes
  to the alignment; a size larger than the room is cut to the room (so the stored size can be less than asked);
  −1 asks for everything (0x40000000); 0..7 become 8 but other negative sizes stay (not exercised). The kept
  bytes and the name move with the end (`Mem_Move`). More than 0x40 bytes left over go back as a free block.
- `EAMem_GetName` 0x2cd9b8 (the name's address: past the data and the kept bytes, 0 if none),
  `EAMem_GetSize` 0x2cd9b0, `EAMem_HeapHeaderSize` 0x2cd990 (the kept bytes), `EAMem_BlockHeaderSize` 0x2cc9a0
  (kept bytes + name and 0 in a named heap), `EAMem_InitBlock` 0x2cd168.
- Misuse writes over the next header, in the game as here: "BEND" (0x200) with under 4 kept bytes, the 0x800
  zeros with under 16, a name (0x100 in the allocation's flags) in a heap that doesn't name its blocks (only the
  heap's own 0x100 counts the name's room). The test keeps to valid use.
- Host: the lock (`RecMutex_Create` 0x2cf468, `RecMutex_Lock` 0x2cf4d0, `RecMutex_Unlock` 0x2cf550); our port is
  single-threaded, `init_heap` takes the handle, and the out-of-memory function is run by the caller (a closure).
- Checked in the runner: 400 heaps with random settings, 48,000 steps (allocations, sizes −1 and negative included, through all four entries with
  every flag, alignment and offset, frees, resizes, the four small getters), 5,340 failed allocations, 1,982
  out-of-memory calls with retries, 8,217 frees joining a neighbour; arena, table and answers equal after each.
  44 mutants: 40 killed; 4 left: `EAMem_InitBlock`'s answer past the name (no caller reads it), the heap flag
  0x400 copied into blocks (no heap sets it), `EAMem_Free` storing the previous block's link it already has, and
  the largest-block search's start (span − 1 or span: differs only for a free block exactly 0x10 bigger than the
  span, not reached).

## The C library's string and memory functions (row E2l; `game/src/libc.rs`, `tests/libc.rs`, proven)

- The game's `strlen` 0x2fd260, `strcpy` 0x2fd14c, `strcmp` 0x2fd008, `strncmp` 0x2fd478, `strncpy` 0x2fd630,
  `memcpy` 0x2fb2b4, `memset` 0x2fb468 and `memcmp` 0x2fb220 are newlib's rewritten in Emotion Engine assembly
  (read from the disassembly; Ghidra's C for them is muddled by the MMI): with both pointers 16-byte aligned
  they go a quadword (`lq`/`sq`, the 0-byte test with `psubb`/`pnor`/`pand`) at a time, 8-byte aligned a
  doubleword (`ld`/`sd`, `dsubu`/`nor`/`and`); `memcpy` and `memset` need 32 bytes or more for their 32-byte
  loop and then go on in doublewords.
- The blocks change nothing that can be seen, so all of them are byte loops in `libc.rs`: the comparisons and
  `strlen` only skip blocks that are equal or hold no 0; a copy reads a block whole and then writes it, and since
  both pointers are aligned to the block they are a whole number of blocks apart, so even a copy onto its own
  source a block ahead leaves what a byte loop leaves (a first port kept the block steps: 12 mutants of them
  survived the test below, which is the proof by trial). `strcmp`, `strncmp` and `memcmp` return the difference
  of the first unequal bytes taken as unsigned.
- `strncpy`'s first quadword (16-byte aligned, n ≥ 16) is tested with `pnor` before the `psubb`, so the test is
  (~x − 1) & ~x & 0x80 per byte instead of (x − 1) & ~x & 0x80: it lets the quadword through only when every
  byte is 0x7f or more, which text never is, so aligned text is copied a byte at a time. After a quadword that
  went through, the next one is tested properly and the copy goes on in doublewords. Only the speed differs.
- `stricmp` 0x2fcc98 and `strnicmp` 0x2fd398 lower a character with newlib's `_ctype_` (0x3ba520; bit 0 of
  `_ctype_[1 + c]` = upper case): inside the loop the character is a signed byte, so bytes from 0x80 up read the
  0x7f bytes before the table (C++ type-info names and the assert message: 'o', 'i', 'e' and other odd letters
  set bit 0 and make the byte 0x20 larger); at the end, for the answer, unsigned (the table's upper half, all 0
  but 0x3ba619 = 0x43 for 0xf8). `stricmp` walks while the first string goes on; `strnicmp` stops at either
  string's end or after `n`.
- The runner lacked `pcpyh` (MMI3, sa 0x1b; `memset` builds its pattern with it): added to
  `tools/r5900/src/exec.rs` with a unit test.

Checked in the runner (`tests/libc.rs`): 80,000 calls, 8,000 per function, on random bytes of four kinds (text
with bytes from 0x80 up, long runs with almost no 0, bytes 0x7f and up, any), mostly 8- or 16-byte aligned,
second strings made from the first with one change, a case swap or a cut, copies onto their own source (5,591), a first string ending where the second has 0xe0:
every byte and every answer equal. 38 hand mutants: 24 killed, the 12 of the first port's block steps (see
above) and 2 equivalent (`strncpy` stopping at the 0 only while some `n` is left: the next step stops anyway;
`strnicmp` looping while `left > 0` instead of `!= -1`: the loop's own `left == 0` test stops it at the same
place).

## The C library's float functions (row E2m; `game/src/libc.rs`, `tests/libc.rs`, proven)

- The game's `fmodf` 0x2f2908 (`__ieee754_fmodf` 0x2f2b90), `logf` 0x2f2a48 (`__ieee754_logf` 0x2f2de0) and
  `isnanf` 0x2f4200 are newlib fdlibm compiled for the EE FPU (`ps2f::F`: no NaNs, infinities or denormals), with
  `_LIB_VERSION` (0x3b9f98) = `1` (`_XOPEN_`) and the default `matherr` 0x2f40d0 (calls `__cmpdf2` 0x2f64b0 on
  `exc.arg1` and returns `0`), writing `errno` = `0x21` (`EDOM`) via `__getreent` 0x2fa4e0 (`_impure_ptr`
  0x35e9ec -> 0x35e700; no game function reads `errno`).
- `isnanf` 0x2f4200 reads the raw 32-bit pattern via `mfc1` (`|x| > 0x7f80_0000`), bypassing the EE FPU.
- `__ieee754_fmodf` 0x2f2b90 is pure integer bit arithmetic for finite `x` and `y != 0` (including subnormal
  inputs and subnormal remainders); for `y == 0`, `|x| >= inf` or `|y| > inf` it evaluates `(x * y) / (x * y)` on
  the EE FPU (`+f32::MAX` `0x7f7f_ffff` when `x * y` flushes to 0, `1.0` otherwise).
- `fmodf` 0x2f2908 calls `__ieee754_fmodf`, then checks `!isnanf(y) && !isnanf(x)` and `c.eq.s y, 0.0` — on the
  EE FPU `load(y) == 0.0` is true for both `±0.0` **and every subnormal `y`**, taking the domain-error path
  (`errno = 0x21`) and returning `__truncdfsf2(0x7ff8_0000_0000_0000)` = `0x7fb0_0000` (not `0x7fc0_0000`: GCC's
  `fp-bit` `__unpack_d` 0x2f5be8 shifts bit 51 to bit 21 and `__pack_f` 0x2f6f08 ORs quiet bit `0x0010_0000`).
- `__ieee754_logf` 0x2f2de0 returns `-inf` (`0xff80_0000`) for `±0.0`, `(x - x) / 0.0` (`+f32::MAX`) for negative
  bit patterns (including negative NaNs), multiplies positive subnormals by `2^25` (`0x4c00_0000`, which flushes
  to `0.0` on the EE FPU, yielding `(-152) * LN2_HI + (-152) * LN2_LO`), returns `x + x` (`+f32::MAX`) for
  `+inf` / positive NaNs, and otherwise evaluates newlib's degree-7 rational polynomial (`LN2_HI` `0x3f31_7180`,
  `LN2_LO` `0x3717_f7d1`, `Lg1..Lg7` `0x3f2a_aaab`..`0x3e17_8897`) in `ps2f::F`.
- `logf` 0x2f2a48 calls `__ieee754_logf`, then checks `!isnanf(x)` and `c.lt.s 0.0, x`: when false on the EE FPU
  (`x <= 0.0`, `-inf`, or any subnormal `x`), sets `errno = 0x21` and returns `__truncdfsf2(__subdf3(0.0, +inf))`
  = `-inf` (`0xff80_0000`).
- `musicsong::ms_to_next_beat` (`Song_MsToNextBeat` 0x224918, `fmodf`'s sole caller in `SLUS_203.26`) and
  `texman::calc_lod_params` (`TexMan_CalcLodParams` 0x1c0da8, `logf`'s sole caller) now call `libc::fmodf` and
  `libc::logf` directly; `musichost.rs` drops its `FMODF` stand-in, and `tests/musicsong.rs`, `tests/musicsys.rs`
  and `tests/texman.rs` run the original `fmodf` / `logf` unstubbed.

Checked in the runner (`tests/libc.rs`, `tests/musicsong.rs`, `tests/musicsys.rs`, `tests/texman.rs`): 21,225 `fmodf` / `__ieee754_fmodf`
pairs (all 35 × 35 edge pairs + 20,000 random pairs across normal, subnormal, multiple, huge-gap and song-ms
ranges; 1,511 domain errors, 1,453 subnormal remainders) and 34,513 `logf` / `__ieee754_logf` values (all edges,
every exponent's `±20`-ULP power-of-two window and `0x30a3d0`/`0x3504e0`/`0x35c288` branch boundaries + 20,000
random values; 1,952 domain/singularity errors), checking both raw bits and `errno` (`0x35e700`): every bit equal.

## The rest of the memory library (row E2n; `game/src/eamem.rs`, `tests/eamem_rest.rs`, proven)

- Globals (gp 0x3c38f0): `gEAMem_MainHeapBase` 0x3bb97c (gp − 0x7f74), `gEAMem_AllocFlags` 0x3bb978 (− 0x7f78,
  the same word the archives and the front end set for a while), `gEAMem_AllocName` 0x3bb98c (− 0x7f64),
  `gEAMem_CheckError` 0x3bb9ac (− 0x7f44). "RAM" is at 0x3bb9a8.
- `EAMem_InitMainHeap` 0x2cd798(base, size): `EAMem_InitHeap(0, "RAM", base, size, 0x40, 0x80, 0, no BEND;
  stack: 0, names 0, lock 1, word 0x2cd790)`, then table slot 1 = slot 0 and the main heap base set if it was 0.
  Called by `Iop_RebootAndLoadModules` (0x182b28).
- `EAMem_InitDefaultHeap` 0x2cd820(size): `Sys_AddExitHandler` 0x2ce9d0(`EAMem_DestroyAll`), `malloc` 0x2fa818,
  the base word set to what it gave (0 too), the main heap made there and the pointer kept at record +0x40.
  `EAMem_InitDefaultHeapMax` 0x2cd890 asks for 0x6000000. Neither is called by the game.
- `EAMem_DestroyHeap` 0x2cd500(id): `RecMutex_Destroy` 0x2cf4b0 on the lock if any, the 0x44-byte record zeroed,
  the slot cleared; 1, or 0 with no heap. `EAMem_DestroyAll` 0x2cd8b0 (the exit handler): first clears every
  slot j ≠ i holding the same record as slot i (in i, j order), then destroys each heap, reading its +0x40 first
  and handing it to `free` 0x2fa840 when not 0; the base word cleared. `EAMem_ClearHeapTable` 0x2cd250 zeroes
  the table (uncalled). `EAMem_Zero` 0x2cc938 is `EAMem_Set(p, 0, n)` (`Speech_LoadNarration`).
- `EAMem_FreeAll` 0x2cd570(id) (uncalled): `EAMem_Free` on every block without the end flag, along the next
  links, each link read **after** the free. It frees free blocks again (the list gets a self-link) and, after a
  block was joined to the free block behind it, walks into that block's stale header; only a heap with no free
  block survives it. Ported as is; in the test it is run on random heaps and memory still matches.
- `Compress_Alloc` 0x2cc958(size) = `EAMem_Alloc(gEAMem_AllocName, size, gEAMem_AllocFlags)`, `Compress_Free`
  0x2cc980 = `EAMem_Free` (both from `Compress_Block` / `Huff_CreateHeader`).
- The checks (`EAMem_ValidateAll` 0x2cdfd8 from `Init` 0x17e9c0 and a virtual at 0x111420):
  - `EAMem_IsBlockHeader` 0x2cdc60(p): 1 for p ≠ 0 (`Mem_IsReadable` 0x2ce2c0 only tests that) tagged FB or SB;
    else error 1.
  - `EAMem_CheckNeighbours` 0x2cdcc8(b): SB → 2 with flag 0x8000, else error 2; FB → 0 (no error) without the
    free flag, else IsBlockHeader of its next then previous free block; MB → 2 without 0x200, else the 4 bytes
    after the data read big-endian must be "BEND" (1) or error 3; any other tag (a 0x424f block) error 4.
  - `EAMem_CheckBackLinks` 0x2cddc0(b): the same SB / tag cases; FB: the next free block's +0x14 and the
    previous one's +0x10 must be b; MB: the next block's +0xc and the previous block's +8; else error 5.
  - `EAMem_CheckInHeap` 0x2cde70(sentinel): along +0x10 past every word with `& 0x4000ffff == 0x40004246`, the
    block reached must be SB, then back along +0x14 the same way to the sentinel; else error 6. A cycle of free
    blocks loops for ever.
  - `EAMem_ValidateHeap` 0x2cdf08(id): under the heap's lock, from the base block along +8, CheckNeighbours and
    CheckBackLinks on each until one gives 0, then CheckInHeap. `EAMem_ValidateAll` runs it on each heap in
    slot order until one fails (1 with no heaps).
- Host: the locks, `Sys_AddExitHandler`, `malloc`, `free` (closures in the port).
- Checked in the runner: 6,000 cases on one to three random heaps (made and used by our E2j code), a third of
  them with broken tags, flags, links, sizes or "BEND" bytes before a check; every error code 0–6 reached;
  43 cases skipped where the original never returns (a broken free list) and ours is not run.
  38 mutants: 36 killed; 2 equivalent (`EAMem_DestroyAll`'s `!= 0` on the repeated record only stops writing 0
  over 0, and comparing j < i too finds nothing, since slot j's own pass already cleared those).

## The `.aip` / `.sop` loaders on random files (row E2o; `tricky_data::aip`, `game/tests/aip_random.rs`, proven)

- `AIP_LoadAIPaths` 0x198518(set, data): count → set+0; `EAMem_Alloc("…", count × 0x40)` filled with 0xdeadc0ed,
  each record's +0x34 = `cAIPath` vtable 0x38abd8, the array → set+4; then the start positions (count, then
  indices) stored as addresses at set+0x14 + 4k, **with no limit**: the seventh lands on set+0x2c, the loaded
  file's pointer (real files have six). Then `AIPath_Load` on each. `AIP_LoadEventPaths` 0x1983d0: count →
  set+8, 0x3c-byte records filled likewise (`cEventPath` vtable 0x38ac10) → set+0xc, set+0x10 = the array + main index × 0x3c
  (any index, unchecked). Both return the bytes read; `AIP_LoadPathsFromFile` ignores that and steps by each
  section's size (a section that reads past its size has the next section's header read from inside it).
- A second section of a kind loads again over the set's count and pointer (ours replaces the paths; start
  positions are written over from the first, further old ones staying, which ours keeps as indices; repeated
  AI-path sections are not in the test).
- `AIPath_Load` 0x196f28: the class's vf2 (defaults: `cAIPath` 0x196700 +0x38 = 50, +0x3c = 1; `cEventPath`
  0x196990 +0x38 = 0), then for each parameter the class's vf3 (`cAIPath` 0x196718: id 100 → +0x38, 101 → +0x3c;
  `cEventPath` 0x196998: id 0 → +0x38), each copying the 4 bytes at the parameter's data **whatever its size** (so
  a shorter parameter gives the bytes after it) and a later one of the same id overwriting the earlier. Fixed in
  `tricky_data::aip`: `AipPath::param` now gives the last one's word, kept in `param_words` (it gave the first,
  and nothing for one shorter than 4 bytes).
- `AIPath_LoadPoint` 0x197130 copies a point onto itself; `AIPath_LoadEvent` 0x197170 rewrites the type in place as
  its place in the table at 0x337e70 (the first match; 0 when not found, the same as −1).
- Negative counts: the game stores them (a path's +0 / +8, the set's counts) and loads nothing; ours keeps no count,
  so they are not in the test.
- Checked in the runner: 3,000 random files (11,694 paths, 19,060 events, 2,665 paths with a repeated parameter,
  6,619 parameters shorter than a word, sections cut short, unknown kinds, main indices anywhere): the set and every
  record compared. 10 mutants of `tricky_data::aip`, all killed.

## The path set's small functions (row E2p; `game/src/aipathset.rs`, `aipath.rs`, `tests/aipathset.rs`, proven)

- The set (0x337fb8): +0 AI paths' count, +4 their 0x40-byte records, +8 event paths' count, +0xc their 0x3c-byte
  records, +0x10 the main event path, +0x14 the start positions, +0x2c the loaded file.
- `AIP_GetPathIndex` 0x198688(set, path) looks among the **event** paths (+8 / +0xc, 0x3c apart) and
  `AIP_GetLineIndex` 0x1986d8 among the **AI** paths (+0 / +4, 0x40 apart), the names notwithstanding: the rider's
  +0x57a0 (its race line) and +0x57a4 (its AI path) in `Boarder_SaveSnapshot`. −1 for 0 or no match (a count
  read as signed). `snapshot::boarder_save` now uses them (they were run through the runner).
- `AIPathSet_GetMainPath` 0x197ef0 (+0x10), `AIPathSet_GetPath` 0x197ef8(set, k) (+0x14 + 4k, unchecked k).
- `AIPath_FindEvent` 0x1977f0(path, kind): the first event (+4, 16 bytes each, +0 their count) whose type (its place
  in the event table) is `kind`, or 0; `aipath::Path::find_event` (used by `race.rs` for the lap event 0xd).
- `AIPath_Free` 0x197e90(set) (from `Course_Unload`): `Mem_Free` 0x14a040 on +4, +0xc, +0x2c when not 0, each
  cleared; the counts stay.
- `cPath::vf1` 0x197b90(path, from f12, to f13): an event of type 0x1f with neither `to < start` nor `end < from`
  (`Path::fork_between`, now written that way: the same for numbers, it matters only for NaN).
  `cPath::vf2` 0x197ce8 / `vf3` 0x197ce0 are empty.
- `AIPathSet_SortByDistance` 0x197f08 is `AIP_SortLinesByDistance` with respawnable = 1 (t0):
  `aipath::sort_set_by_distance`.
- Checked in the runner: 20,000 random sets (counts negative too, record pointers 0, events touching the range's
  ends exactly), answers, memory and the frees compared; `tests/snapshot_riders.rs` now runs the original two
  index functions on a set with a few paths. 16 mutants, all killed.
