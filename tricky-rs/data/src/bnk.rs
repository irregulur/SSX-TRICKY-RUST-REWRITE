//! EA `BNKl` sound bank reader and sample decoder (board row E1b2g7d3b7b2, `SND_ParsePTLayer` 0x2d5410,
//! `SLUS_203.26`).
//!
//! Layout (proven from `SND_ParsePTLayer` 0x2d5410 and verified against `DATA/AUDIO/AUDIO.BIG` `Crowd.bnk`):
//! - Header: `"BNKl"`, `version: u16` (5), `nprog: u16`, `hdr_size: u32`, `data_size: u32`, `reserved: u32`.
//! - `0x14 + 4 * i`: `u32` offset relative to the slot's own address (`0` = empty program).
//! - Each program: `"PT"`, `u16` version (5), then tag stream:
//!   - `0xfc`: 1-byte pad; `0xfd`: start sample sub-header; `0xfe`: next layer; `0xff`: end.
//!   - Otherwise: `tag: u8`, `len: u8`, big-endian `len`-byte value.
//! - Codecs (tag `0xa0`, default `5`):
//!   - `5`: PS2 SPU2 VAG ADPCM (16-byte blocks of 28 samples).
//!   - `9`: signed 8-bit PCM after a 16-byte inline header.
//!   - `0x0a`: EA-XA ADPCM (15-byte frames of 28 samples, `0xee` = raw 61-byte frame).

use std::collections::BTreeMap;

/// One decoded layer of a bank program (`SND_ParsePTLayer` 0x2d5410 defaults).
#[derive(Clone, Debug)]
pub struct Layer {
    pub vel_lo: u8,
    pub vel_hi: u8,
    pub key_lo: u8,
    pub key_hi: u8,
    pub root: u8,
    pub bend_range: u8,
    pub pan: u8,
    pub vol: u8,
    pub detune: i32,
    pub rate: u32,
    pub channels: u8,
    pub codec: u8,
    pub loop_start: i32,
    pub loop_end: i32,
    /// Decoded 16-bit PCM samples (interleaved when `channels > 1`).
    pub samples: Vec<i16>,
}

/// One program (one or more layers) inside a `BNKl` bank.
#[derive(Clone, Debug)]
pub struct Program {
    pub layers: Vec<Layer>,
}

/// Parsed EA `BNKl` sound bank.
#[derive(Clone, Debug)]
pub struct Bank {
    pub version: u16,
    pub nprog: u16,
    pub programs: BTreeMap<usize, Program>,
}

const VAG_COEF: [(i32, i32); 5] = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)];
const EAXA_COEF: [(i32, i32); 4] = [(0, 0), (240, 0), (460, -208), (392, -220)];

/// Decode PS2 SPU2 VAG ADPCM blocks into `count` 16-bit PCM samples.
pub fn decode_vag(data: &[u8], mut off: usize, count: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(count);
    let (mut h1, mut h2) = (0i32, 0i32);
    while out.len() < count && off + 16 <= data.len() {
        let shift = (data[off] & 0x0f) as u32;
        let filter = ((data[off] >> 4) & 0x07).min(4) as usize;
        let (c1, c2) = VAG_COEF[filter];
        for &b in &data[off + 2..off + 16] {
            for nib in [b & 0x0f, b >> 4] {
                let raw = if nib > 7 { (nib as i32 - 16) << 12 } else { (nib as i32) << 12 };
                let s = ((raw >> shift) + ((h1 * c1 + h2 * c2 + 32) >> 6)).clamp(-32768, 32767);
                out.push(s as i16);
                h2 = h1;
                h1 = s;
                if out.len() == count {
                    return out;
                }
            }
        }
        off += 16;
    }
    out
}

/// Decode EA-XA ADPCM frames into `count` 16-bit PCM samples.
pub fn decode_eaxa(data: &[u8], mut off: usize, count: usize) -> Vec<i16> {
    let mut out = Vec::with_capacity(count);
    let (mut h1, mut h2) = (0i32, 0i32);
    while out.len() < count && off < data.len() {
        let info = data[off];
        if info == 0xee {
            if off + 61 > data.len() {
                break;
            }
            h1 = i16::from_be_bytes([data[off + 1], data[off + 2]]) as i32;
            h2 = i16::from_be_bytes([data[off + 3], data[off + 4]]) as i32;
            for i in 0..28 {
                let p = off + 5 + i * 2;
                let s = i16::from_be_bytes([data[p], data[p + 1]]);
                out.push(s);
                h2 = h1;
                h1 = s as i32;
                if out.len() == count {
                    return out;
                }
            }
            off += 61;
            continue;
        }
        if off + 15 > data.len() {
            break;
        }
        let (c1, c2) = EAXA_COEF[((info >> 4) & 3) as usize];
        let shift = ((info & 0x0f) + 8) as u32;
        for i in 0..28 {
            let b = data[off + 1 + (i >> 1)];
            let nib = if (i & 1) == 0 { b >> 4 } else { b & 0x0f };
            let raw = if (nib & 8) != 0 {
                ((nib as i32) << 28).wrapping_sub(1 << 28 << 4)
            } else {
                (nib as i32) << 28
            };
            let s = (((raw >> shift) + c1 * h1 + c2 * h2 + 128) >> 8).clamp(-32768, 32767);
            out.push(s as i16);
            h2 = h1;
            h1 = s;
            if out.len() == count {
                return out;
            }
        }
        off += 15;
    }
    out
}

fn read_be_u64(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for &x in b {
        v = (v << 8) | x as u64;
    }
    v
}

fn parse_program(data: &[u8], mut off: usize) -> Option<Program> {
    if off + 4 > data.len() || &data[off..off + 2] != b"PT" {
        return None;
    }
    off += 4;
    let mut raw_layers: Vec<(BTreeMap<u8, u64>, Option<BTreeMap<u8, u64>>)> = vec![(BTreeMap::new(), None)];
    let mut in_sample = false;
    while off < data.len() {
        let tag = data[off];
        off += 1;
        match tag {
            0xfc => continue,
            0xff => break,
            0xfe => {
                raw_layers.push((BTreeMap::new(), None));
                in_sample = false;
                continue;
            }
            0xfd => {
                raw_layers.last_mut().unwrap().1 = Some(BTreeMap::new());
                in_sample = true;
                continue;
            }
            _ => {}
        }
        let len = *data.get(off)? as usize;
        off += 1;
        let val = read_be_u64(data.get(off..off + len)?);
        off += len;
        let last = raw_layers.last_mut().unwrap();
        if in_sample {
            if let Some(smp) = &mut last.1 {
                smp.insert(tag, val);
            }
        } else {
            last.0.insert(tag, val);
        }
    }

    let mut layers = Vec::new();
    for (pt, smp) in raw_layers {
        let Some(smp) = smp else { continue };
        let count = *smp.get(&0x85).unwrap_or(&0) as usize;
        let data_off = *smp.get(&0x88).unwrap_or(&0) as usize;
        let channels = (*smp.get(&0x82).unwrap_or(&1) as u8).max(1);
        let codec = *smp.get(&0xa0).unwrap_or(&5) as u8;
        let rate = *smp.get(&0x84).unwrap_or(&22050) as u32;
        let loop_start = *smp.get(&0x86).unwrap_or(&(u32::MAX as u64)) as u32 as i32;
        let loop_end = *smp.get(&0x87).unwrap_or(&(u32::MAX as u64)) as u32 as i32;

        let mut chan_offs = vec![data_off];
        for c in 1..channels as usize {
            let tag = 0x89u8.wrapping_add((c - 1) as u8);
            chan_offs.push(*smp.get(&tag).unwrap_or(&0) as usize);
        }
        let mut per_chan: Vec<Vec<i16>> = Vec::with_capacity(channels as usize);
        for &o in &chan_offs {
            let pcm = match codec {
                9 => {
                    let start = o.saturating_add(16);
                    let end = start.saturating_add(count).min(data.len());
                    data.get(start..end)
                        .unwrap_or(&[])
                        .iter()
                        .map(|&b| ((b as i8) as i16) << 8)
                        .collect()
                }
                0x0a => decode_eaxa(data, o, count),
                _ => decode_vag(data, o, count),
            };
            per_chan.push(pcm);
        }
        let frames = per_chan.iter().map(Vec::len).min().unwrap_or(0);
        let samples = if channels == 1 {
            per_chan.into_iter().next().unwrap_or_default()
        } else {
            let mut inter = Vec::with_capacity(frames * channels as usize);
            for i in 0..frames {
                for ch in &per_chan {
                    inter.push(ch[i]);
                }
            }
            inter
        };
        let detune_raw = *pt.get(&0x10).unwrap_or(&0) as u32 as i32;
        layers.push(Layer {
            vel_lo: *pt.get(&0x01).unwrap_or(&0) as u8,
            vel_hi: *pt.get(&0x02).unwrap_or(&127) as u8,
            key_lo: *pt.get(&0x03).unwrap_or(&0) as u8,
            key_hi: *pt.get(&0x04).unwrap_or(&127) as u8,
            root: *pt.get(&0x07).unwrap_or(&60) as u8,
            bend_range: *pt.get(&0x0a).unwrap_or(&0) as u8,
            pan: *pt.get(&0x0c).unwrap_or(&64) as u8,
            vol: *pt.get(&0x0e).unwrap_or(&127) as u8,
            detune: detune_raw,
            rate,
            channels,
            codec,
            loop_start,
            loop_end,
            samples,
        });
    }
    Some(Program { layers })
}

/// Parse an EA `BNKl` sound bank and decode all of its sample layers to 16-bit PCM.
pub fn parse(data: &[u8]) -> Option<Bank> {
    if data.len() < 20 || &data[0..4] != b"BNKl" {
        return None;
    }
    let version = u16::from_le_bytes([data[4], data[5]]);
    let nprog = u16::from_le_bytes([data[6], data[7]]);
    let mut programs = BTreeMap::new();
    for i in 0..nprog as usize {
        let slot = 20 + 4 * i;
        if slot + 4 > data.len() {
            break;
        }
        let rel = u32::from_le_bytes([data[slot], data[slot + 1], data[slot + 2], data[slot + 3]]);
        if rel == 0 {
            continue;
        }
        let target = slot.checked_add(rel as usize)?;
        if let Some(prog) = parse_program(data, target) {
            if !prog.layers.is_empty() {
                programs.insert(i, prog);
            }
        }
    }
    Some(Bank { version, nprog, programs })
}

impl Layer {
    /// Whether this layer defines a valid loop range.
    pub fn loops(&self) -> bool {
        self.loop_start >= 0 && self.loop_end > self.loop_start
    }

    /// Duration in seconds at nominal playback rate (`rate`).
    pub fn duration_secs(&self) -> f32 {
        let frames = self.samples.len() / (self.channels.max(1) as usize);
        if self.rate == 0 { 0.0 } else { frames as f32 / self.rate as f32 }
    }

    /// Encode this layer's PCM samples (trimmed to `loop_end + 1` when looping) as a standard RIFF WAV.
    pub fn wav(&self) -> Vec<u8> {
        let ch = self.channels.max(1) as usize;
        let frames = self.samples.len() / ch;
        let end = if self.loop_end >= 0 { ((self.loop_end as usize) + 1).min(frames) } else { frames };
        encode_wav(&self.samples[..end * ch], self.rate, self.channels.max(1))
    }

    /// Encode just the loop region `[loop_start..=loop_end]` as a RIFF WAV, if this layer loops.
    pub fn loop_wav(&self) -> Option<Vec<u8>> {
        if !self.loops() {
            return None;
        }
        let ch = self.channels.max(1) as usize;
        let frames = self.samples.len() / ch;
        let a = (self.loop_start as usize).min(frames);
        let b = ((self.loop_end as usize) + 1).min(frames);
        if b <= a {
            return None;
        }
        Some(encode_wav(&self.samples[a * ch..b * ch], self.rate, self.channels.max(1)))
    }
}

fn encode_wav(samples: &[i16], rate: u32, channels: u8) -> Vec<u8> {
    let ch = channels as u16;
    let data = (samples.len() * 2) as u32;
    let mut w = Vec::with_capacity(44 + data as usize);
    w.extend_from_slice(b"RIFF");
    w.extend_from_slice(&(36 + data).to_le_bytes());
    w.extend_from_slice(b"WAVEfmt ");
    w.extend_from_slice(&16u32.to_le_bytes());
    w.extend_from_slice(&1u16.to_le_bytes());
    w.extend_from_slice(&ch.to_le_bytes());
    w.extend_from_slice(&rate.to_le_bytes());
    w.extend_from_slice(&(rate * 2 * ch as u32).to_le_bytes());
    w.extend_from_slice(&(2 * ch).to_le_bytes());
    w.extend_from_slice(&16u16.to_le_bytes());
    w.extend_from_slice(b"data");
    w.extend_from_slice(&data.to_le_bytes());
    for &s in samples {
        w.extend_from_slice(&s.to_le_bytes());
    }
    w
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_crowd_bnk_from_disc() {
        let Ok(disc_path) = std::env::var("TRICKY_DISC") else { return };
        let disc = crate::disc::Disc::open(&disc_path).expect("open disc");
        let mut files = crate::big::Files::new(disc);
        files.mount_game_archives().expect("mount");
        let raw = files
            .load(r"|data\audio\Crowd.bnk")
            .expect("load")
            .expect("Crowd.bnk missing from AUDIO.BIG");
        let bank = parse(&raw).expect("parse Crowd.bnk");
        assert_eq!(bank.version, 5);
        assert_eq!(bank.programs.keys().copied().collect::<Vec<_>>(), vec![0, 1, 2, 16, 17, 18]);
        for (&prog_id, prog) in &bank.programs {
            assert_eq!(prog.layers.len(), 1, "prog {prog_id}");
            let l = &prog.layers[0];
            assert!(l.samples.len() > 10_000, "prog {prog_id} too short");
            assert!(l.samples.iter().any(|&s| s.abs() > 1000), "prog {prog_id} silent");
            let wav = l.wav();
            assert_eq!(&wav[0..4], b"RIFF");
            assert_eq!(&wav[8..12], b"WAVE");
        }
        // Crowd bed programs 0..=2 loop; cheer/ahh/anticipation programs 16..=18 are one-shots.
        for p in [0, 1, 2] {
            assert!(bank.programs[&p].layers[0].loops(), "bed {p} should loop");
            assert!(bank.programs[&p].layers[0].loop_wav().is_some());
        }
        for p in [16, 17, 18] {
            assert!(!bank.programs[&p].layers[0].loops(), "cheer {p} should be one-shot");
        }
    }
}
