use crate::TempoTransaction;

impl reth_primitives_traits::InMemorySize for TempoTransaction {
    fn size(&self) -> usize {
        Self::size(self)
    }
}

#[cfg(feature = "reth-codec")]
impl reth_codecs::Compact for TempoTransaction {
    fn to_compact<B>(&self, buf: &mut B) -> usize
    where
        B: alloy_rlp::BufMut + AsMut<[u8]>,
    {
        use alloy_rlp::Encodable;
        self.encode(buf);
        self.length()
    }

    fn from_compact(mut buf: &[u8], _len: usize) -> (Self, &[u8]) {
        let item = alloy_rlp::Decodable::decode(&mut buf)
            .expect("Failed to decode TempoTransaction from compact");
        (item, buf)
    }
}
