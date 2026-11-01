//! # Scalar backend
//!
//! The reference backend: table-lookup kernels for the driver, and the steps
//! that alone handle padding, stray bytes, partial quanta, the carry between
//! calls and the end of an input.

use core::mem::MaybeUninit;

pub(crate) const ALPHABET: &[u8; 64] =
    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

pub(crate) const LINE: usize = 76;
pub(crate) const LINE_BYTES: usize = LINE / 4 * 3;

/// Bytes `decode_bytes` takes before handing back to the runs mid-line.
const RESYNC: usize = 64;

/// The class of `=`, the lowest of the bytes outside the alphabet.
const PAD: u8 = 64;
const NEWLINE: u8 = 65;
const BLANK: u8 = 66;
const BAD: u8 = 67;

/// Every byte's sextet, or its class outside the alphabet.
static CLASSES: [u8; 256] = {
    let mut classes = [BAD; 256];
    let mut sextet = 0;

    while sextet < 64 {
        classes[ALPHABET[sextet] as usize] = sextet as u8;
        sextet += 1;
    }

    classes[b'=' as usize] = PAD;
    classes[b'\n' as usize] = NEWLINE;
    classes[b'\r' as usize] = BLANK;
    classes[b' ' as usize] = BLANK;
    classes[b'\t' as usize] = BLANK;
    classes
};

/// Per position in a quantum, every byte's sextet in place in its 3 bytes,
/// or the high byte set outside the alphabet: four lookups joined decode it.
static QUANTA: [[u32; 256]; 4] = {
    let mut quanta = [[0xff00_0000; 256]; 4];
    let mut sextet = 0;

    while sextet < 64 {
        let mut at = 0;

        while at < 4 {
            quanta[at][ALPHABET[sextet] as usize] = ((sextet as u32) << (26 - 6 * at)).swap_bytes();
            at += 1;
        }

        sextet += 1;
    }

    quanta
};

/// The characters of every pair of sextets, the first one high.
static PAIRS: [[u8; 2]; 4096] = {
    let mut pairs = [[0; 2]; 4096];
    let mut at = 0;

    while at < 4096 {
        pairs[at] = [ALPHABET[at >> 6], ALPHABET[at & 63]];
        at += 1;
    }

    pairs
};

base64_driver!(
    decode(
        chars: 16,
        spare: 4,
        kernel: decode_block,
        narrow: decode_quanta,
    ),
    encode(
        bytes: 12,
        window: 12,
        lead: 0,
        kernel: encode_block,
        narrow: encode_groups,
    ),
);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct DecodeState {
    pub(crate) bits: u32,
    pub(crate) pending: u8,
    pub(crate) padded: bool,
    pub(crate) clean: bool,
}

impl DecodeState {
    pub(crate) const fn new() -> Self {
        Self {
            bits: 0,
            pending: 0,
            padded: false,
            clean: true,
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct EncodeState {
    pub(crate) carry: [u8; 2],
    pub(crate) carried: u8,
    pub(crate) column: u8,
    pub(crate) eol: &'static [u8],
}

impl EncodeState {
    pub(crate) const fn new(eol: &'static [u8]) -> Self {
        Self {
            carry: [0; 2],
            carried: 0,
            column: 0,
            eol,
        }
    }

    /// The bytes `encode_bytes` takes before the loops: up to the next line
    /// when wrapped, else the next group.
    pub(crate) fn lead(&self) -> usize {
        let carried = self.carried as usize;

        if self.eol.is_empty() {
            return (3 - carried) % 3;
        }

        match self.column as usize / 4 * 3 + carried {
            0 => 0,
            at => LINE_BYTES - at,
        }
    }
}

/// The length of the line ending `src` starts with, 0 for none.
pub(crate) fn eol(src: &[u8]) -> usize {
    match src {
        [b'\r', b'\n', ..] => 2,
        [b'\n', ..] => 1,
        _ => 0,
    }
}

/// Decodes `src` byte by byte with every lenient rule, done at its end or at
/// a full `dst`, not done where the runs can take over.
pub(crate) fn decode_bytes(
    state: &mut DecodeState,
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
) -> (usize, usize, bool) {
    let DecodeState {
        mut bits,
        mut pending,
        mut padded,
        mut clean,
    } = *state;

    let mut written = 0;
    let mut line_start = false;

    let (read, done) = 'bytes: {
        for (read, &byte) in src.iter().enumerate() {
            if read > 0 && pending == 0 && !padded && (line_start || read >= RESYNC) {
                break 'bytes (read, false);
            }

            match CLASSES[byte as usize] {
                PAD => padded = true,
                NEWLINE | BLANK => {}
                BAD => clean = false,
                sextet if padded => {
                    let Some(len) = flush(bits, pending, &mut dst[written..]) else {
                        break 'bytes (read, true);
                    };

                    written += len;
                    clean = false;
                    padded = false;
                    bits = sextet as u32;
                    pending = 1;
                }
                sextet if pending == 3 => {
                    let Some(out) = dst
                        .get_mut(written..)
                        .and_then(|rest| rest.first_chunk_mut())
                    else {
                        break 'bytes (read, true);
                    };

                    let [_, first, second, third] = (bits << 6 | sextet as u32).to_be_bytes();
                    *out = [first, second, third].map(MaybeUninit::new);
                    written += 3;
                    bits = 0;
                    pending = 0;
                }
                sextet => {
                    bits = bits << 6 | sextet as u32;
                    pending += 1;
                }
            }

            line_start = byte == b'\n';
        }

        (src.len(), true)
    };

    *state = DecodeState {
        bits,
        pending,
        padded,
        clean,
    };

    (read, written, done)
}

/// Takes the at most three `symbols` before an input's trimmed end into an
/// empty `state`, or returns false when one is not a symbol.
pub(crate) fn decode_tail(state: &mut DecodeState, symbols: &[u8], padded: bool) -> bool {
    let mut bits = 0;

    for &byte in symbols {
        let sextet = CLASSES[byte as usize];

        if sextet >= PAD {
            return false;
        }

        bits = bits << 6 | sextet as u32;
    }

    state.bits = bits;
    state.pending = symbols.len() as u8;
    state.padded = padded;
    true
}

/// The length of `src` without the padding and blanks ending it, and whether
/// they hold padding.
pub(crate) fn trimmed(src: &[u8]) -> (usize, bool) {
    let mut end = src.len();
    let mut padded = false;

    while let [rest @ .., last @ (b'=' | b'\n' | b'\r' | b' ' | b'\t')] = &src[..end] {
        padded |= *last == b'=';
        end = rest.len();
    }

    (end, padded)
}

pub(crate) fn decode_finish(state: &mut DecodeState, dst: &mut [MaybeUninit<u8>]) -> Option<usize> {
    let len = flush(state.bits, state.pending, dst)?;

    if state.pending == 1 {
        state.clean = false;
    }

    state.bits = 0;
    state.pending = 0;
    Some(len)
}

/// Encodes `src` group by group until one does not fit in `dst`, ending full
/// lines when wrapped and carrying the last one or two bytes.
pub(crate) fn encode_bytes(
    state: &mut EncodeState,
    src: &[u8],
    dst: &mut [MaybeUninit<u8>],
) -> (usize, usize) {
    let mut read = 0;
    let mut written = 0;

    loop {
        let carried = state.carried as usize;
        let rest = &src[read..];

        if carried + rest.len() < 3 {
            for (slot, byte) in state.carry[carried..].iter_mut().zip(rest) {
                *slot = *byte;
            }

            state.carried += rest.len() as u8;
            return (src.len(), written);
        }

        let ends_line = !state.eol.is_empty() && state.column as usize + 4 == LINE;
        let len = 4 + if ends_line { state.eol.len() } else { 0 };

        let Some(out) = dst.get_mut(written..written + len) else {
            return (read, written);
        };

        let [first, second] = state.carry;

        let group = match carried {
            0 => [rest[0], rest[1], rest[2]],
            1 => [first, rest[0], rest[1]],
            _ => [first, second, rest[0]],
        };

        let (chars, end) = out.split_at_mut(4);
        encode_groups(&group, chars);

        if !state.eol.is_empty() {
            state.column += 4;
        }

        if ends_line {
            write_eol(end, state.eol);
            state.column = 0;
        }

        state.carried = 0;
        read += 3 - carried;
        written += len;
    }
}

pub(crate) fn encode_finish(state: &mut EncodeState, dst: &mut [MaybeUninit<u8>]) -> Option<usize> {
    let chars = if state.carried == 0 { 0 } else { 4 };

    let eol = if state.column as usize + chars == 0 {
        0
    } else {
        state.eol.len()
    };

    let out = dst.get_mut(..chars + eol)?;
    let (quantum, end) = out.split_at_mut(chars);

    encode_tail(&state.carry[..state.carried as usize], quantum);
    write_eol(end, state.eol);
    state.carried = 0;
    state.column = 0;
    Some(chars + eol)
}

/// Writes the padded quantum of the last one or two bytes, when it fits.
pub(crate) fn encode_tail(tail: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
    let quantum = match *tail {
        [first] => [
            ALPHABET[(first >> 2) as usize],
            ALPHABET[(first << 4 & 63) as usize],
            b'=',
            b'=',
        ],
        [first, second] => [
            ALPHABET[(first >> 2) as usize],
            ALPHABET[((first << 4 | second >> 4) & 63) as usize],
            ALPHABET[(second << 2 & 63) as usize],
            b'=',
        ],
        _ => return 0,
    };

    let Some(out) = dst.first_chunk_mut::<4>() else {
        return 0;
    };

    *out = quantum.map(MaybeUninit::new);
    4
}

/// Writes `eol` at the start of `out` when it fits, without a copy call.
pub(crate) fn write_eol(out: &mut [MaybeUninit<u8>], eol: &[u8]) {
    match (eol, out) {
        ([cr, lf], [first, second, ..]) => {
            first.write(*cr);
            second.write(*lf);
        }
        ([lf], [first, ..]) => {
            first.write(*lf);
        }
        _ => {}
    }
}

/// A byte per quantum of the blocks met, joined, set when it held a byte
/// outside the alphabet.
#[derive(Clone, Copy)]
struct Bad(u32);

impl Bad {
    const EAGER: bool = false;

    fn none() -> Self {
        Self(0)
    }

    fn join(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    fn any(self) -> bool {
        self.0 != 0
    }

    /// The characters of the whole quanta before the first bad one.
    fn first(self) -> usize {
        self.0.trailing_zeros() as usize / 8 * 4
    }
}

/// Decodes a block of 16 characters into 12 bytes, each quantum stored as a
/// word whose spare byte the next one overwrites.
#[inline]
fn decode_block<const C: usize, const N: usize>(
    chars: &[u8; C],
    split: usize,
    out: &mut [MaybeUninit<u8>; N],
) -> Bad {
    let (quads, _) = chars.as_chunks::<4>();
    let mut bad = 0;

    for (at, mut quad) in quads.iter().take(4).enumerate() {
        if C > 16 && at * 4 >= split {
            quad = chars[at * 4 + C - 16..].first_chunk().unwrap_or(quad);
        }

        let word = quantum(quad);
        let len = (N - at * 3).min(4);
        out[at * 3..at * 3 + len].copy_from_slice(&word.to_le_bytes().map(MaybeUninit::new)[..len]);
        bad |= (word >> 24) << (8 * at);
    }

    Bad(bad)
}

/// Encodes `K` groups of 12 bytes into 16 characters each.
#[inline]
fn encode_block<const S: usize, const N: usize, const K: usize>(
    bytes: &[u8; S],
    split: usize,
    outs: [&mut [MaybeUninit<u8>; N]; K],
) {
    for (group, out) in bytes.as_chunks::<12>().0.iter().zip(outs) {
        encode_groups(group, &mut out[..16]);

        if N > 16 {
            crate::driver::spread(out, split, 16);
        }
    }
}

/// `Loops::decode_run` quantum by quantum.
pub(crate) fn decode_quanta(src: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
    let (quads, _) = src.as_chunks::<4>();
    let (outs, _) = dst.as_chunks_mut::<3>();
    let mut read = 0;

    for (quad, out) in quads.iter().zip(outs) {
        let [first, second, third, bad] = quantum(quad).to_le_bytes();

        if bad != 0 {
            break;
        }

        *out = [first, second, third].map(MaybeUninit::new);
        read += 4;
    }

    read
}

/// `Loops::encode_run` two groups a step, a pair of sextets a lookup.
#[inline]
pub(crate) fn encode_groups(src: &[u8], dst: &mut [MaybeUninit<u8>]) -> usize {
    let (groups, _) = src.as_chunks::<6>();
    let (outs, _) = dst.as_chunks_mut::<8>();
    let mut read = 0;

    for (&[a, b, c, d, e, f], out) in groups.iter().zip(outs) {
        let bits = u64::from_be_bytes([0, 0, a, b, c, d, e, f]);
        let [p, q, r, s] = [36, 24, 12, 0].map(|at| PAIRS[(bits >> at & 0xfff) as usize]);
        *out = [p[0], p[1], q[0], q[1], r[0], r[1], s[0], s[1]].map(MaybeUninit::new);
        read += 6;
    }

    if let (Some(&[a, b, c]), Some(out)) = (
        src[read..].first_chunk(),
        dst[read / 3 * 4..].first_chunk_mut(),
    ) {
        let bits = u32::from_be_bytes([0, a, b, c]);
        let [p, q] = [12, 0].map(|at| PAIRS[(bits >> at & 0xfff) as usize]);
        *out = [p[0], p[1], q[0], q[1]].map(MaybeUninit::new);
        read += 3;
    }

    read
}

/// The 3 bytes of a quantum in a little-endian word, the high byte set when
/// a character is outside the alphabet.
fn quantum(&[a, b, c, d]: &[u8; 4]) -> u32 {
    let [first, second, third, fourth] = &QUANTA;
    first[a as usize] | second[b as usize] | third[c as usize] | fourth[d as usize]
}

/// Writes the bytes of a partial quantum of `pending` sextets, a lone one
/// dropped, or none when they do not fit.
fn flush(bits: u32, pending: u8, out: &mut [MaybeUninit<u8>]) -> Option<usize> {
    match (pending, out) {
        (2, [first, ..]) => {
            first.write((bits >> 4) as u8);
            Some(1)
        }
        (3, [first, second, ..]) => {
            first.write((bits >> 10) as u8);
            second.write((bits >> 2) as u8);
            Some(2)
        }
        (2 | 3, _) => None,
        _ => Some(0),
    }
}

/// Checks a decode kernel against `decode_quanta` for every byte at every
/// position, and for a stray byte around CRLF at every split.
#[cfg(test)]
pub(crate) fn check_decode_kernel<const S: usize, const B: usize>(
    kernel: impl Fn(&[u8; S], usize, &mut [MaybeUninit<u8>; B]) -> usize,
) {
    let valid: alloc::vec::Vec<u8> = (0..S - 2).map(|at| ALPHABET[(at * 37 + 11) % 64]).collect();
    let bytes = (0..S - 2).flat_map(|at| (0..=255).map(move |byte| (at, byte, S - 2)));
    let splits = (4..S - 2)
        .step_by(4)
        .flat_map(|split| (0..=S - 2).map(move |at| (at, b'*', split)));

    for (at, byte, split) in bytes.chain(splits) {
        let mut chars = valid.clone();

        if let Some(slot) = chars.get_mut(at) {
            *slot = byte;
        }

        let window = [&chars[..split], b"\r\n", &chars[split..]].concat();
        let mut out = [MaybeUninit::new(0); B];
        let mut expected = [MaybeUninit::new(0); B];
        let read = kernel(window.as_slice().try_into().unwrap(), split, &mut out);
        assert_eq!(
            read,
            decode_quanta(&chars, &mut expected),
            "{byte} at {at}, {split}"
        );

        // SAFETY: every slot was initialised before the calls.
        let (out, expected) = unsafe {
            (
                out.map(|b| b.assume_init()),
                expected.map(|b| b.assume_init()),
            )
        };

        assert_eq!(
            out[..read / 4 * 3],
            expected[..read / 4 * 3],
            "{byte} at {at}, {split}"
        );
    }
}

/// Checks an encode kernel against `encode_groups` for every byte at every
/// position, and for CRLF at every split.
#[cfg(test)]
pub(crate) fn check_encode_kernel<const W: usize, const N: usize>(
    lead: usize,
    kernel: impl Fn(&[u8; W], usize, &mut [MaybeUninit<u8>; N]),
) {
    let chars = N - 2;
    let base: [u8; W] = core::array::from_fn(|at| (at * 73 + 5) as u8);
    let bytes = (0..W).flat_map(|at| (0..=255).map(move |byte| (at, byte, chars)));
    let splits = (4..chars).step_by(4).map(|split| (0, base[0], split));

    for (at, byte, split) in bytes.chain(splits) {
        let mut bytes = base;
        bytes[at] = byte;

        let mut out = [MaybeUninit::new(0); N];
        let mut expected = [MaybeUninit::new(0); N];
        kernel(&bytes, split, &mut out);
        encode_groups(&bytes[lead..lead + chars / 4 * 3], &mut expected);

        // SAFETY: every slot was initialised before the calls.
        let (out, expected) = unsafe {
            (
                out.map(|b| b.assume_init()),
                expected.map(|b| b.assume_init()),
            )
        };

        assert_eq!(out[..split], expected[..split], "{byte} at {at}, {split}");
        assert_eq!(out[split..split + 2], *b"\r\n", "{byte} at {at}, {split}");
        assert_eq!(
            out[split + 2..],
            expected[split..chars],
            "{byte} at {at}, {split}"
        );
    }
}

#[cfg(test)]
mod tests {
    use crate::scalar::*;

    #[test]
    fn decode_quanta_stops_at_every_byte_outside_the_alphabet() {
        for byte in 0..=255 {
            let mut chars = *b"QUJDQUJD";
            chars[6] = byte;
            let mut out = [MaybeUninit::new(0); 6];
            let expected = if ALPHABET.contains(&byte) { 8 } else { 4 };
            assert_eq!(decode_quanta(&chars, &mut out), expected, "{byte}");
        }
    }

    #[test]
    fn kernels_match_the_quantum_and_group_loops() {
        check_decode_kernel(|chars, split, out| run(decode_block::<18, 12>(chars, split, out)));
        check_decode_kernel(|chars, split, out: &mut [MaybeUninit<u8>; 12]| {
            let mut wide = [MaybeUninit::new(0); 16];
            let read = run(decode_block::<18, 16>(chars, split, &mut wide));
            out.copy_from_slice(&wide[..12]);
            read
        });
        check_encode_kernel(
            0,
            |bytes: &[u8; 12], split, out: &mut [MaybeUninit<u8>; 18]| {
                encode_block(bytes, split, [out])
            },
        );
    }

    fn run(bad: Bad) -> usize {
        if bad.any() { bad.first() } else { 16 }
    }
}
