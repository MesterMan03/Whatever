use super::schema::MAX_BYTES;
use anyhow::{Result, bail, ensure};
use quinn::{RecvStream, SendStream};

pub async fn read_frame(stream: &mut RecvStream) -> Result<Option<Vec<u8>>> {
    let mut length = 0u32;
    for i in 0..5 {
        let mut byte = [0];
        let read = stream.read(&mut byte).await?;
        if read.is_none() && i == 0 {
            return Ok(None);
        }
        ensure!(read == Some(1), "truncated frame length");
        let b = byte[0];
        ensure!(i != 4 || b <= 15, "frame length overflow");
        length |= u32::from(b & 127) << (i * 7);
        if b & 128 == 0 {
            ensure!(i == 0 || b != 0, "noncanonical frame length");
            let length = length as usize;
            ensure!((2..=MAX_BYTES).contains(&length), "invalid frame size");
            let mut bytes = vec![0; length];
            stream.read_exact(&mut bytes).await?;
            return Ok(Some(bytes));
        }
    }
    bail!("frame length overflow")
}
pub async fn write_frame(stream: &mut SendStream, bytes: &[u8]) -> Result<()> {
    ensure!((2..=MAX_BYTES).contains(&bytes.len()), "invalid frame size");
    let mut prefix = Vec::with_capacity(5);
    let mut length = bytes.len();
    loop {
        let byte = (length & 127) as u8;
        length >>= 7;
        prefix.push(byte | if length != 0 { 128 } else { 0 });
        if length == 0 {
            break;
        }
    }
    stream.write_all(&prefix).await?;
    stream.write_all(bytes).await?;
    Ok(())
}
