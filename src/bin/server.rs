use std::net::SocketAddr;
use tokio::{
    io::{
        self, AsyncReadExt, AsyncWriteExt
    },
    net::{TcpListener, TcpStream},
    sync::{broadcast, oneshot}
};

// passed between tasks
#[derive(Debug, Clone)]
struct Message {
    // used so that a task doesn't echo data back to the client
    from_id: tokio::task::Id,
    from: String,
    data: String
}

impl Message {
    fn new(from_id: tokio::task::Id, from: String, data: String) -> Self {
        Self {from_id, from, data}
    }
}


async fn handle_client(client: TcpStream, _address: SocketAddr, tx: broadcast::Sender<Message>) {
    // split stream into buffered reader and writer
    let (rd, mut wr) = io::split(client);

    // receive messages from other tasks here
    let mut msg_rx = tx.subscribe();
    
    // second task will check for a signal before sending to client
    // makes the 2 tasks quit at roughly the same time
    let (_exit_tx, exit_rx) = oneshot::channel::<bool>();
    
    let task_id = tokio::task::id();

    tokio::spawn(async move {
        // handle receiving data from client then sending to others
        loop {
        };
    });

    tokio::spawn(async move {
        // handle receiving data from broadcast and send to client=
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
                            if msg.from_id == task_id {
                                continue;
                            }
                            println!("{task_id} sending data to client");
                            if let Err(e) = wr.write_all(msg.data.as_bytes()).await {
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
    let (tx, _) = broadcast::channel::<Message>(32);

    let listener = TcpListener::bind("127.0.0.1:8034").await?;

    loop {
        let (sock, addr) = listener.accept().await?;

        let tx = tx.clone();

        tokio::spawn(async move {
            handle_client(sock, addr, tx).await;
        });
    }
}