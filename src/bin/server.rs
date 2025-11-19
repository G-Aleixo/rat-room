use std::net::SocketAddr;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::{broadcast, oneshot}
};

use rat_room::message::Message;

fn handle_client(client: TcpStream, _address: SocketAddr, tx: broadcast::Sender<(Message, uuid::Uuid)>) {
    println!("client handler function started");
    // split stream into buffered reader and writer
    let (mut rd, mut wr) = io::split(client);

    // receive messages from other tasks here
    let mut msg_rx = tx.subscribe();
    
    // second task will check for a signal before sending to client
    // makes the 2 tasks quit at roughly the same time
    let (exit_tx, exit_rx) = oneshot::channel::<bool>();
    
    let pair_uuid = uuid::Uuid::new_v4();

    tokio::spawn(async move {
        // handle reading data from client then sending to others
        loop {
            if let Ok(msg) = Message::read(&mut rd).await {
                println!("client handler {pair_uuid} has received message '{msg}'");
                tx.send((msg, pair_uuid)).unwrap();
            } else {
                println!("read/send task {pair_uuid} has failed to receive message");
                // send kill signal to other program
                exit_tx.send(false).unwrap();

                break;
            }
        };
    });

    tokio::spawn(async move {
        // handle receiving data from broadcast and write to client
        tokio::select! {
            //todo: refactor this thing later
            result = exit_rx => {
                match result {
                    Ok(true) => {
                        println!("recv/write task {pair_uuid} shutting down");
                        //todo: shut down other stuff
                    }
                    Ok(false) => {
                        println!("recv/write task {pair_uuid} has erroed somehow, shutting down");
                        //todo: shut down due to error
                    }
                    Err(_) => println!("recv/write task {pair_uuid} failed to receive rx value, shutting down")
                }
            }

            _ = async {
                loop {
                    match msg_rx.recv().await {
                        Ok(msg) => {
                            if msg.1 == pair_uuid {
                                continue;
                            }
                            println!("{pair_uuid} sending data to client");
                            if let Err(e) = msg.0.write(&mut wr).await {
                                eprintln!("{pair_uuid} failed to write to client: {e}");
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
async fn main() -> io::Result<()> {
    let (tx, _) = broadcast::channel::<(Message, uuid::Uuid)>(32);

    println!("starting socket");
    let listener = TcpListener::bind("127.0.0.1:8080").await?;

    println!("starting listening loop");
    loop {
        let (sock, addr) = listener.accept().await?;
        println!("client accepted at {addr}");

        let tx = tx.clone();

        println!("spawning client handler function");
        // client handler just spawns some more tasks
        handle_client(sock, addr, tx);
    }
}