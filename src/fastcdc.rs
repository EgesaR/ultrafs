pub struct Chunk {
    pub offset: usize,
    pub length: usize,
}

pub struct FastCDC {
    min_size: usize,
    avg_size: usize,
    max_size: usize,
    mask_s: u64,
    mask_l: u64,
}

impl FastCDC {
    /// Configures FastCDC with sub-4KB chunk limits.
    /// Default setup: Min 512B, Avg 2048B (2KB), Max 4096B (4KB)
    pub fn new(min_size: usize, avg_size: usize, max_size: usize) -> Self {
        let bits = (avg_size as f64).log2().round() as u32;
        let mask_s = (1u64 << (bits - 1)) - 1;
        let mask_l = (1u64 << (bits + 1)) - 1;

        Self {
            min_size,
            avg_size,
            max_size,
            mask_s,
            mask_l,
        }
    }

    pub fn chunkify<'a>(&self, data: &'a [u8]) -> Vec<Chunk> {
        let mut chunks = Vec::new();
        let mut offset = 0;
        let len = data.len();

        while offset < len {
            if len - offset <= self.min_size {
                chunks.push(Chunk {
                    offset,
                    length: len - offset,
                });
                break;
            }

            let max_chunk = (len - offset).min(self.max_size);
            let mut hash = 0u64;
            let mut cut_point = max_chunk;

            let start = offset + self.min_size;
            let avg_point = offset + self.avg_size;
            let end = offset + max_chunk;

            for i in start..end {
                let byte = data[i];
                hash = (hash << 1).wrapping_add(GEAR_TABLE[byte as usize]);

                let mask = if i < avg_point {
                    self.mask_s
                } else {
                    self.mask_l
                };

                if (hash & mask) == 0 {
                    cut_point = i - offset + 1;
                    break;
                }
            }

            chunks.push(Chunk {
                offset,
                length: cut_point,
            });

            offset += cut_point;
        }

        chunks
    }
}

// Standard 256-entry Gear Table (pseudo-random 64-bit integers)
const GEAR_TABLE: [u64; 256] = [
    0x0000000000000000,
    0x12b2628d00000000,
    0x2564c51a00000000,
    0x37d6a79700000000,
    0x4ac98a3400000000,
    0x587be8b900000000,
    0x6fad4f2e00000000,
    0x7d1f2da300000000,
    0x9593146800000000,
    0x872176e500000000,
    0xb0f7d17200000000,
    0xa245b3ff00000000,
    0xdf5a9e5c00000000,
    0xcd28fc3100000000,
    0xfa3e5b4600000000,
    0xe88c39cb00000000,
    0x2b2628d000000000,
    0x39944a5d00000000,
    0x0e42edca00000000,
    0x1cf08f4700000000,
    0x61efa2e400000000,
    0x735dc06900000000,
    0x448b67fe00000000,
    0x5639057300000000,
    0xbebedcb800000000,
    0xac0cbe3500000000,
    0x9bd119a200000000,
    0x89637b2f00000000,
    0xf127568c00000000,
    0xe395340100000000,
    0xd443939600000000,
    0xc6f1f11b00000000,
    0x564c51a000000000,
    0x44fe332d00000000,
    0x732894ba00000000,
    0x619af63700000000,
    0x1c85db9400000000,
    0x0e37b91900000000,
    0x39e11e8e00000000,
    0x2b537c0300000000,
    0xc3df45c800000000,
    0xd16d274500000000,
    0xe6bb80d200000000,
    0xf409e25f00000000,
    0x8916cf1c00000000,
    0x9baaa09100000000,
    0xac720a0600000000,
    0xbe80688b00000000,
    0x7d6a797000000000,
    0x6fd81bfd00000000,
    0x580ebc6a00000000,
    0x4abcdee700000000,
    0x37a3f34400000000,
    0x251191c900000000,
    0x12c7365e00000000,
    0x007554d300000000,
    0xe8f96d1800000000,
    0xfaab0f9500000000,
    0xcd9da80200000000,
    0xdf2fc88f00000000,
    0xa230e72c00000000,
    0xb08285a100000000,
    0x8754223600000000,
    0x95e640bb00000000,
    0xac98a34000000000,
    0xbe2ac1cd00000000,
    0x89fc665a00000000,
    0x9b4e04d700000000,
    0xe651297400000000,
    0xf4e34bf900000000,
    0xc335ec6e00000000,
    0xd1878ee300000000,
    0x390bb72800000000,
    0x2bb9d5a500000000,
    0x1c6f723200000000,
    0x0ed110bf00000000,
    0x73c23d1c00000000,
    0x61705f9100000000,
    0x56a6f80600000000,
    0x44149a8b00000000,
    0x87be8b9000000000,
    0x950ce91d00000000,
    0xa2da4e8a00000000,
    0xb0682c0700000000,
    0xcd7701a400000000,
    0xdfc5632900000000,
    0xe813c4be00000000,
    0xfaa1a63300000000,
    0x122d9ff800000000,
    0x009ff77500000000,
    0x37495ae200000000,
    0x25fb386f00000000,
    0x58e415cc00000000,
    0x4a56774100000000,
    0x7d80d0d600000000,
    0x6f32b25b00000000,
    0xfad4f2e000000000,
    0xe866906d00000000,
    0xdfb037fa00000000,
    0xcd02557700000000,
    0xb01d78d400000000,
    0xa2af1a5900000000,
    0x9579bdce00000000,
    0x87cbdf4300000000,
    0x6f47e68800000000,
    0x7df5840500000000,
    0x4a23239200000000,
    0x5891411f00000000,
    0x258e6cbc00000000,
    0x373c0e3100000000,
    0x00eaa9a600000000,
    0x1258cb2b00000000,
    0xd1f2da3000000000,
    0xc340b8bd00000000,
    0xf4961f2a00000000,
    0xe6247da700000000,
    0x9b3b500400000000,
    0x8989328900000000,
    0xbe5f951e00000000,
    0xacedf79300000000,
    0x4461ce5800000000,
    0x56d3ac3500000000,
    0x61050b4200000000,
    0x73b769cf00000000,
    0x0ea8446c00000000,
    0x1c1a26e100000000,
    0x2bcce17600000000,
    0x397ebdfb00000000,
    0x5931468000000000,
    0x4b83240d00000000,
    0x7c55839a00000000,
    0x6ee7e11700000000,
    0x13f8ccb400000000,
    0x014aa23900000000,
    0x369c09ae00000000,
    0x242e6b2300000000,
    0xcc2252e800000000,
    0xde90306500000000,
    0xe94697f200000000,
    0xfbf4f57f00000000,
    0x86ebdcbc00000000,
    0x9459be3100000000,
    0xa38f19a600000000,
    0xb13d7b2b00000000,
    0x72176e5000000000,
    0x60a50cd300000000,
    0x5773ab4a00000000,
    0x45c1c9c700000000,
    0x38dee46400000000,
    0x2a6c86eb00000000,
    0x1dba217e00000000,
    0x0f0843f300000000,
    0xe7847a3800000000,
    0xf53618b500000000,
    0xc2e0bf2200000000,
    0xd052ddaf00000000,
    0xad4df00c00000000,
    0xbfff928100000000,
    0x8829351600000000,
    0x9a9b579b00000000,
    0x0f7d172000000000,
    0x1dcfe5ad00000000,
    0x2a19d23a00000000,
    0x38ab10b700000000,
    0x45b49d1400000000,
    0x5706ff9900000000,
    0x60d0580e00000000,
    0x72623a8300000000,
    0x9aee034800000000,
    0x885c61c500000000,
    0xbf8ac65200000000,
    0xad38a4df00000000,
    0xd027897c00000000,
    0xc295ebf100000000,
    0xf5434c6600000000,
    0xe7f12ebe00000000,
    0x245b3ff000000000,
    0x36e95d7d00000000,
    0x013ff2ea00000000,
    0x138d906700000000,
    0x6e92b5c400000000,
    0x7c20d74900000000,
    0x4bf670de00000000,
    0x5944125300000000,
    0xb1c82b9800000000,
    0xa37a491500000000,
    0x94acee8200000000,
    0x861e8c0f00000000,
    0xfb01a1ac00000000,
    0xe9b3c32100000000,
    0xde6564b600000000,
    0xcc27063b00000000,
    0xf5a9e5c000000000,
    0xe71b874d00000000,
    0xd0cd20da00000000,
    0xc27f425700000000,
    0xbf606ff400000000,
    0xad220d7900000000,
    0x9a04aaee00000000,
    0x88b6c86300000000,
    0x603af1a800000000,
    0x7288932500000000,
    0x455e34b200000000,
    0x57ec563f00000000,
    0x2af37b9c00000000,
    0x3841191100000000,
    0x0f97be8600000000,
    0x1d25dc0b00000000,
    0xde8fc31000000000,
    0xcc3da19d00000000,
    0xfbe2060a00000000,
    0xe950648700000000,
    0x9446492400000000,
    0x86f42ba900000000,
    0xb1228c3e00000000,
    0xa390eeeb00000000,
    0x4b1cd77800000000,
    0x59aeedf500000000,
    0x6e78126200000000,
    0x7cca70ef00000000,
    0x01d55d4c00000000,
    0x13673fc100000000,
    0x2eb1985600000000,
    0x3c03faeb00000000,
    0xa3e5b46000000000,
    0xb157d6ed00000000,
    0x8681717a00000000,
    0x943313f700000000,
    0xe92c3e5400000000,
    0xfb9e5cd900000000,
    0xcc48fb4e00000000,
    0xdefa99c300000000,
    0x3676a00800000000,
    0x24c4c28500000000,
    0x1312651200000000,
    0x01a0079f00000000,
    0x7cbf2a3c00000000,
    0x6ed048b100000000,
    0x59dbef2600000000,
    0x4b698da300000000,
    0x88c39cb000000000,
    0x9a71fe3d00000000,
    0xada759aa00000000,
    0xbf153b2700000000,
    0xc20a168400000000,
    0xd0b8740900000000,
    0xe76ed39e00000000,
    0xf5dc311300000000,
    0x1d5088d800000000,
    0x0fe2ea5500000000,
    0x38344dc200000000,
    0x2a862f4f00000000,
    0x579902ec00000000,
    0x452b606100000000,
    0x72fdc7f600000000,
    0x604fa57b00000000,
];
