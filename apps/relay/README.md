# Graphite relay

A small server that lets devices sync a vault without a shared folder: phones, or desktops on different networks. It stores encrypted objects and tells connected devices when one changes. It never has the key, so it cannot read notes, files or even file names; a relay you host for friends learns only how much data each vault has and when it changes.

## Run it

```
cargo run --release -p graphite-relay
```

or with Docker, from the repository root:

```
docker build -f apps/relay/Dockerfile -t graphite-relay .
docker run -d -p 7474:7474 -v graphite-relay:/data graphite-relay
```

Settings are environment variables: `RELAY_DATA` (where objects live, default `./relay-data`), `RELAY_ADDR` (default `0.0.0.0:7474`), `RELAY_MAX_MB` (largest object accepted, default 1024).

Put it behind HTTPS (Caddy or nginx with a certificate) before using it over the internet; the data is encrypted either way, but the token that lets a device talk to the relay travels in a header.

## Connect a vault

On the first device, in Graphite: open the sync panel from the sidebar, enter the relay's URL and choose "Create a key". Copy the invite it shows and paste it into the sync panel on every other device (or `graphite relay connect '<invite>'` for the CLI). The invite is the relay URL and the vault key; anyone who has it can read the vault, so share it like a password.

The first token seen for a vault is the one the relay keeps accepting, so a vault id cannot be taken over by a later arrival with a different key. To start over for a vault, delete its folder under `RELAY_DATA` and its row in `relay.sqlite`.
