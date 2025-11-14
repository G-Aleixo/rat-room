use tokio::io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    from: String,
    data: String
}

// Message::func
impl Message {
    pub fn new(from: String, data: String) -> Result<Self, String> {
        if from.len() > u8::MAX.into() {
            return Err(format!(
                "'from' field exceeds size {}, being {}",
                u8::MAX, from.len()
            ));
        }
        if data.len() > u8::MAX.into() {
            return Err(format!(
                "'data' field exceeds size {}, being {}",
                u8::MAX, data.len()
            ));
        }

        Ok(Self { from, data })
    }

    pub async fn read<T>(reader: &mut T) -> io::Result<Self>
    where T: AsyncRead + Unpin
    {
        let from_len = reader.read_u8().await? as usize;
        let data_len = reader.read_u8().await? as usize;

        let mut from_buf = vec![0u8; from_len];
        reader.read_exact(&mut from_buf).await?;
        let from = String::from_utf8(from_buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 in 'from'"))?;

        let mut data_buf = vec![0u8; data_len];
        reader.read_exact(&mut data_buf).await?;
        let data = String::from_utf8(data_buf)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Invalid UTF-8 in 'data'"))?;

        Ok(Self { from, data })
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }

        let from_len = bytes[0] as usize;
        let data_len = bytes[1] as usize;

        if bytes.len() < 2 + from_len + data_len {
            return None;
        }

        let from_start = 2;
        let from_end = from_start + from_len;
        let data_end = from_end + data_len;

        let from = String::from_utf8(bytes[from_start..from_end].to_vec()).ok()?;
        let data = String::from_utf8(bytes[from_end..data_end].to_vec()).ok()?;

        Some(Self { from, data })
    }
}

// Message.func
impl Message {
    pub async fn write<T>(&self, writer: &mut T) -> io::Result<()>
    where T: AsyncWrite + Unpin
    {
        let bytes = self.to_bytes();

        writer.write_all(&bytes).await?;

        Ok(())
    }
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        let from_bytes = self.from.as_bytes();
        let data_bytes = self.data.as_bytes();

        buf.push(from_bytes.len() as u8);
        buf.push(data_bytes.len() as u8);

        buf.extend_from_slice(from_bytes);
        buf.extend_from_slice(data_bytes);

        buf
    }
}