use std::net::SocketAddr;
use tokio::{
    io,
    net::{TcpListener, TcpStream},
    sync::{broadcast, oneshot}, task
};

use rat_room::message::Message;

fn handle_client(client: TcpStream, _address: SocketAddr, tx: broadcast::Sender<(Message, task::Id)>) {
    println!("client handler function started");
    // split stream into buffered reader and writer
    let (mut rd, mut wr) = io::split(client);

    // receive messages from other tasks here
    let mut msg_rx = tx.subscribe();
    
    // second task will check for a signal before sending to client
    // makes the 2 tasks quit at roughly the same time
    let (exit_tx, exit_rx) = oneshot::channel::<bool>();
    
    let task_id = tokio::task::id();

    tokio::spawn(async move {
        // handle reading data from client then sending to others
        loop {
            if let Ok(msg) = Message::read(&mut rd).await {
                println!("client handler {task_id} has received message '{msg}'");
                tx.send((msg, task_id)).unwrap();
            } else {
                println!("read/send task {task_id} has failed to receive message");
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
async fn main() -> io::Result<()> {
    let (tx, _) = broadcast::channel::<(Message, task::Id)>(32);

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