use std::net::SocketAddr;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::{broadcast, oneshot}, task
};

use rat_room::message::Message;

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
                            if let Err(e) = msg.0.write(&mut wr).await {
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