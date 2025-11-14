use std::net::SocketAddr;
use tokio::{
    io::{
        self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadHalf, WriteHalf
    },
    net::{TcpListener, TcpStream},
    sync::{broadcast, oneshot}, task
};

// passed between tasks
#[derive(Debug, Clone, PartialEq)]
struct Message {
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

    async fn read<T>(reader: &mut T) -> io::Result<Self>
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

    fn from_bytes(bytes: &[u8]) -> Option<Self> {
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
    async fn write<T>(&self, writer: &mut T) -> io::Result<()>
    where T: AsyncWrite + Unpin
    {
        let bytes = self.to_bytes();

        writer.write_all(&bytes).await?;

        Ok(())
    }
    fn to_bytes(&self) -> Vec<u8> {
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


async fn handle_client(client: TcpStream, _address: SocketAddr, tx: broadcast::Sender<(Message, task::Id)>) {
    // split stream into buffered reader and writer
    let (rd, mut wr) = io::split(client);

    // receive messages from other tasks here
    let mut msg_rx = tx.subscribe();
    
    // second task will check for a signal before sending to client
    // makes the 2 tasks quit at roughly the same time
    let (exit_tx, exit_rx) = oneshot::channel::<bool>();
    
    let task_id = tokio::task::id();

    tokio::spawn(async move {
        // handle reading data from client then sending to others
        loop {

        };
    });

    tokio::spawn(async move {
        // handle receiving data from broadcast and write to client
        tokio::select! {
            //todo: refactor this thing later
            result = exit_rx => {
                match result {
                    Ok(true) => {
                        println!("recv/write task {task_id} shutting down");
                        //todo: shut down other stuff
                    }
                    Ok(false) => {
                        println!("recv/write task {task_id} has erroed somehow, shutting down");
                        //todo: shut down due to error
                    }
                    Err(_) => println!("recv/write task {task_id} failed to receive rx value, shutting down")
                }
            }

            _ = async {
                loop {
                    match msg_rx.recv().await {
                        Ok(msg) => {
                            if msg.1 == task_id {
                                continue;
                            }
                            println!("{task_id} sending data to client");
                            if let Err(e) = wr.write_all(&msg.0.to_bytes()).await {
                                eprintln!("{task_id} failed to write to client: {e}");
                                break;
                            }
                        }
                        Err(_) => break, // broadcast closed
                    }
                }
            } => {}
        }
    });
}

#[tokio::main]
async fn main() -> io::Result<()>{
    let (tx, _) = broadcast::channel::<(Message, task::Id)>(32);

    let listener = TcpListener::bind("127.0.0.1:8034").await?;

    loop {
        let (sock, addr) = listener.accept().await?;

        let tx = tx.clone();

        tokio::spawn(async move {
            handle_client(sock, addr, tx).await;
        });
    }
}