# Connecting Render to a machine at home (Tailscale)

The plan is to run the database on a home PC instead of paying for one on Render.
[Tailscale](https://tailscale.com) puts the Render server and the PC on a private network
(a *tailnet*), so the PC never has to be exposed to the internet.

```
Render container                                          Home PC
┌─────────────────────────────────────────┐               ┌──────────────────┐
│ backend ──SOCKS5──▶ tailscaled          │══ tailnet ═══▶│ Tailscale        │
│        localhost:1055  (userspace mode) │  (encrypted)  │ database :5432   │
└─────────────────────────────────────────┘               └──────────────────┘
```

Render containers can't create network devices, so `tailscaled` runs in
[userspace mode](https://tailscale.com/kb/1112/userspace-networking). The backend reaches tailnet
machines through the SOCKS5 proxy it opens on `localhost:1055`.
[`scripts/render-start.sh`](../scripts/render-start.sh) handles this, and it only joins the
tailnet when `TS_AUTHKEY` is set.

## One-time setup

1. **Create a Tailscale account** at <https://login.tailscale.com/start>. The free Personal plan is
   enough.
2. **Install Tailscale on the PC** that will host the database, and sign in with that account.
3. **Set the access rules.** In the admin console, open **Access controls** and replace the
   policy with the one below. People in the tailnet can still reach everything, but the Render
   server can *only* open connections to port 5432 (Postgres) on your devices, nothing else.

   ```jsonc
   {
     // Who is allowed to hand out the tag the Render server uses.
     "tagOwners": {
       "tag:render": ["autogroup:admin"]
     },
     "grants": [
       // People (and their devices) can reach every device.
       { "src": ["autogroup:member"], "dst": ["*"], "ip": ["*"] },
       // The Render server can only reach the database port on people's devices.
       { "src": ["tag:render"], "dst": ["autogroup:member"], "ip": ["tcp:5432"] }
     ]
   }
   ```

4. **Create a credential for Render.** Go to **Settings → Trust credentials**, click
   **Credential → OAuth**, give it the **Auth Keys** scope with **Write** access, and add the
   tag `tag:render`. Copy the secret (it starts with `tskey-client-`; it's only shown once).
   It doesn't expire, unlike auth keys, which last at most 90 days.
5. **Give it to Render.** In the Render dashboard, open the service, go to **Environment**, and
   add `TS_AUTHKEY` with the secret as its value. Saving redeploys the service.
6. **Check it worked.**
   - Render's **Logs** should show `tailscale: joined the tailnet as 100.x.y.z`.
   - On the PC, `tailscale status` lists `footy-render`, and `tailscale ping footy-render`
     gets a pong.

Treat the secret like a password. Keep it out of the repo, chat and screenshots. If it ever
leaks, delete it under Trust credentials and create a new one.

## Good to know

- **Each restart is a new device.** The free plan sleeps after 15 idle minutes, and each wake-up
  joins the tailnet as a fresh *ephemeral* device. If an old one hasn't disappeared yet, the new
  one shows up as `footy-render-1`, and so on. Old ones are removed automatically.
- **When the database arrives:**
  - The PC must be on and awake whenever the site needs data. In Tailscale's settings on the
    PC, turn on **Run unattended**, and in the admin console **disable key expiry** for the PC.
    Otherwise it drops off the tailnet when you log out of Windows, or after 180 days.
  - Let the database accept connections from tailnet addresses (`100.64.0.0/10`), with a
    password. Allow that port through Windows Firewall *only* for that range.
  - The backend will connect via `socks5://localhost:1055` to the PC's Tailscale name or its
    `100.x.y.z` address. Some Rust database drivers (like `sqlx`) can't use a SOCKS5 proxy
    directly, so we'll add a small forwarder when we get there.
