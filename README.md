# [WIP] Quinn Tower

Idea: Implement a simple file transfer service using QUIC to achieve improved performance compared to using SCP for copying the tower file (which contains voting data). The service will copy the file content and store it on a caching server (Cloudflare KV). This ensures that even if the primary node's network card fails, the backup node can use the cached tower file to promote itself effectively.

- [ ] Implement QUIC server to handle connections
- [ ] Implement some db to store the file
- [ ] Make reader so client can read the file
- [ ] Implement cmd so this can be trigger by external service(watcher or something else)
