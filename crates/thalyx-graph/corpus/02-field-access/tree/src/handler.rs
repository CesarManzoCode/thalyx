use crate::server::Server;

pub fn handle(server: &Server) {
    server.store.persist();
}
