#[derive(Clone, Copy)]
pub(super) struct RandnState {
    x: u32,
    y: u32,
    z: u32,
    w: u32,
}

impl RandnState {
    pub(super) fn new() -> Self {
        Self {
            x: 123_456_789,
            y: 362_436_069,
            z: 521_288_629,
            w: 88_675_123,
        }
    }

    #[inline]
    pub(super) fn randn(&mut self) -> f32 {
        let x = self.x;
        let y = self.y;
        let z = self.z;
        let w = self.w;

        let t1 = x ^ (x << 11);
        let t2 = y ^ (y << 11);
        let t3 = z ^ (z << 11);
        let t4 = w ^ (w << 11);

        let w1 = (w ^ (w >> 19)) ^ (t1 ^ (t1 >> 8));
        let w2 = (w1 ^ (w1 >> 19)) ^ (t2 ^ (t2 >> 8));
        let w3 = (w2 ^ (w2 >> 19)) ^ (t3 ^ (t3 >> 8));
        let w4 = (w3 ^ (w3 >> 19)) ^ (t4 ^ (t4 >> 8));

        let t5 = w1 ^ (w1 << 11);
        let t6 = w2 ^ (w2 << 11);
        let t7 = w3 ^ (w3 << 11);
        let t8 = w4 ^ (w4 << 11);

        let w5 = (w4 ^ (w4 >> 19)) ^ (t5 ^ (t5 >> 8));
        let w6 = (w5 ^ (w5 >> 19)) ^ (t6 ^ (t6 >> 8));
        let w7 = (w6 ^ (w6 >> 19)) ^ (t7 ^ (t7 >> 8));
        let w8 = (w7 ^ (w7 >> 19)) ^ (t8 ^ (t8 >> 8));

        let t9 = w5 ^ (w5 << 11);
        let t10 = w6 ^ (w6 << 11);
        let t11 = w7 ^ (w7 << 11);
        let t12 = w8 ^ (w8 << 11);

        let w9 = (w8 ^ (w8 >> 19)) ^ (t9 ^ (t9 >> 8));
        let w10 = (w9 ^ (w9 >> 19)) ^ (t10 ^ (t10 >> 8));
        let w11 = (w10 ^ (w10 >> 19)) ^ (t11 ^ (t11 >> 8));
        let w12 = (w11 ^ (w11 >> 19)) ^ (t12 ^ (t12 >> 8));

        self.x = w9;
        self.y = w10;
        self.z = w11;
        self.w = w12;

        let sum = (w1 >> 4)
            .wrapping_add(w2 >> 4)
            .wrapping_add(w3 >> 4)
            .wrapping_add(w4 >> 4)
            .wrapping_add(w5 >> 4)
            .wrapping_add(w6 >> 4)
            .wrapping_add(w7 >> 4)
            .wrapping_add(w8 >> 4)
            .wrapping_add(w9 >> 4)
            .wrapping_add(w10 >> 4)
            .wrapping_add(w11 >> 4)
            .wrapping_add(w12 >> 4);

        sum as f32 / 268_435_456.0 - 6.0
    }
}
