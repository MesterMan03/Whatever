use super::schema::{CompiledPacket, MAX_BYTES, MAX_ITEMS, MAX_NODES, Schema};
use anyhow::{Result, bail, ensure};
use serde_json::{Map, Value, json};

fn uleb(mut n: u32, out: &mut Vec<u8>) {
    loop {
        let mut b = (n & 127) as u8;
        n >>= 7;
        if n != 0 {
            b |= 128;
        }
        out.push(b);
        if n == 0 {
            break;
        }
    }
}
fn encode_value(schema: &Schema, value: &Value, out: &mut Vec<u8>) -> Result<()> {
    match schema {
        Schema::I32 { .. } => out.extend_from_slice(
            &(value
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("expected number"))? as i32)
                .to_be_bytes(),
        ),
        Schema::F64 { .. } => out.extend_from_slice(
            &value
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("expected number"))?
                .to_be_bytes(),
        ),
        Schema::Varint { .. } => {
            let n = value
                .as_f64()
                .ok_or_else(|| anyhow::anyhow!("expected number"))? as i32;
            uleb(((n as u32) << 1) ^ ((n >> 31) as u32), out);
        }
        Schema::Bool => out.push(u8::from(
            value
                .as_bool()
                .ok_or_else(|| anyhow::anyhow!("expected bool"))?,
        )),
        Schema::String { .. } => {
            let s = value
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("expected string"))?;
            uleb(s.len() as u32, out);
            out.extend_from_slice(s.as_bytes());
        }
        Schema::Optional { inner } => {
            out.push(u8::from(!value.is_null()));
            if !value.is_null() {
                encode_value(inner, value, out)?;
            }
        }
        Schema::Array { element, .. } => {
            let values = value
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("expected array"))?;
            uleb(values.len() as u32, out);
            for v in values {
                encode_value(element, v, out)?;
            }
        }
        Schema::Struct { fields } => {
            for field in fields {
                encode_value(&field.schema, &value[&field.name], out)?;
            }
        }
    }
    ensure!(out.len() <= MAX_BYTES, "packet exceeds {MAX_BYTES} bytes");
    Ok(())
}
pub fn encode(packet: &CompiledPacket, value: &Value) -> Result<Vec<u8>> {
    let value = packet.schema.normalize(value)?;
    let mut bytes = packet.info.id.to_be_bytes().to_vec();
    encode_value(&packet.schema, &value, &mut bytes)?;
    Ok(bytes)
}
struct Reader<'a> {
    bytes: &'a [u8],
    position: usize,
    budget: usize,
}
impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let end = self
            .position
            .checked_add(N)
            .ok_or_else(|| anyhow::anyhow!("length overflow"))?;
        let bytes = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| anyhow::anyhow!("truncated packet"))?;
        self.position = end;
        bytes
            .try_into()
            .map_err(|_| anyhow::anyhow!("invalid fixed-width value"))
    }
    fn length(&mut self) -> Result<u32> {
        let mut n = 0u32;
        for index in 0..5 {
            let b = self.take::<1>()?[0];
            ensure!(index != 4 || b <= 15, "ULEB128 overflows u32");
            n |= u32::from(b & 127) << (index * 7);
            if b & 128 == 0 {
                ensure!(index == 0 || b != 0, "non-canonical ULEB128");
                return Ok(n);
            }
        }
        bail!("unterminated ULEB128")
    }
    fn value(&mut self, schema: &Schema) -> Result<Value> {
        ensure!(self.budget > 0, "decoded value exceeds node limit");
        self.budget -= 1;
        Ok(match schema {
            Schema::I32 { .. } => json!(i32::from_be_bytes(self.take()?)),
            Schema::F64 { .. } => {
                let n = f64::from_be_bytes(self.take()?);
                ensure!(n.is_finite(), "non-finite f64");
                json!(n)
            }
            Schema::Varint { .. } => {
                let n = self.length()?;
                json!(((n >> 1) as i32) ^ -((n & 1) as i32))
            }
            Schema::Bool => json!(self.take::<1>()?[0] != 0),
            Schema::Optional { inner } => {
                if self.take::<1>()?[0] == 0 {
                    Value::Null
                } else {
                    self.value(inner)?
                }
            }
            Schema::String { max_length } => {
                let len = self.length()? as usize;
                ensure!(
                    len <= max_length.unwrap_or(MAX_BYTES),
                    "UTF-8 string exceeds length limit"
                );
                let end = self
                    .position
                    .checked_add(len)
                    .ok_or_else(|| anyhow::anyhow!("length overflow"))?;
                let bytes = self
                    .bytes
                    .get(self.position..end)
                    .ok_or_else(|| anyhow::anyhow!("truncated string"))?;
                let text = std::str::from_utf8(bytes)?;
                self.position = end;
                json!(text)
            }
            Schema::Array {
                element,
                max_length,
            } => {
                let len = self.length()? as usize;
                ensure!(
                    len <= max_length.unwrap_or(MAX_ITEMS) && len <= self.budget,
                    "array exceeds length/node limit"
                );
                let mut values = Vec::with_capacity(len);
                for _ in 0..len {
                    values.push(self.value(element)?);
                }
                Value::Array(values)
            }
            Schema::Struct { fields } => {
                let mut values = Map::new();
                for field in fields {
                    values.insert(field.name.clone(), self.value(&field.schema)?);
                }
                Value::Object(values)
            }
        })
    }
}
pub fn decode(packets: &[CompiledPacket], bytes: &[u8]) -> Result<Value> {
    ensure!(bytes.len() <= MAX_BYTES, "packet exceeds {MAX_BYTES} bytes");
    let mut reader = Reader {
        bytes,
        position: 0,
        budget: MAX_NODES,
    };
    let id = u16::from_be_bytes(reader.take()?);
    let packet = packets
        .get(usize::from(id))
        .ok_or_else(|| anyhow::anyhow!("unknown packet ID {id}"))?;
    let value = reader.value(&packet.schema)?;
    ensure!(reader.position == bytes.len(), "trailing packet bytes");
    let data = packet.schema.normalize(&value)?;
    Ok(
        json!({"packet":packet.info.packet,"state":packet.info.state,"direction":packet.info.direction,"data":data}),
    )
}
