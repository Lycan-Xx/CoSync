import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { QRCodeSVG } from "qrcode.react";
import "./App.css";

type PairingPayload = { device_name: string; public_key_fingerprint: string; ip_hint: string; port: number; pairing_token: string };
type PairedDevice = { device_id: string; device_name: string; cert_fingerprint: string; last_known_ip?: string | null; last_known_port?: number | null; connected: boolean };
type View = "pairing" | "devices";

function StatusDot({ connected }: { connected: boolean }) { return <span className={`status-dot ${connected ? "connected" : "offline"}`} aria-hidden="true" />; }

function PhoneIcon() {
  return <svg className="phone-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true"><rect x="6.5" y="2.5" width="11" height="19" rx="2" stroke="currentColor" strokeWidth="1.7" /><path d="M10 18.5h4" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" /></svg>;
}

function PairingScreen({ onDevices }: { onDevices: () => void }) {
  const [payload, setPayload] = useState<PairingPayload | null>(null);
  const [rawPayload, setRawPayload] = useState("");
  const [hostname, setHostname] = useState("THIS PC");
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    Promise.all([invoke<string>("get_pairing_qr"), invoke<string>("get_hostname")])
      .then(([raw, name]) => { if (!active) return; setRawPayload(raw); setPayload(JSON.parse(raw) as PairingPayload); setHostname(name); })
      .catch((reason) => { if (active) setError(String(reason)); });
    return () => { active = false; };
  }, []);

  return <main className="pairing-screen">
    <header className="window-heading"><div className="brand-mark"><PhoneIcon /></div><span>CoSync — pair</span><button className="text-button" onClick={onDevices}>Paired devices</button></header>
    <section className="pairing-content">
      <p className="eyebrow">THIS PC</p><h1>{hostname}</h1>
      {payload ? <div className="qr-frame"><QRCodeSVG value={rawPayload} size={300} bgColor="#ffffff" fgColor="#111318" level="M" /></div> : <div className="qr-frame qr-placeholder">{error ? "Unable to create pairing QR" : "Preparing secure pairing…"}</div>}
      <p className="pairing-copy">Open CoSync on your phone and scan.<br />Keys pin here — no account, no cloud.</p>
      {payload && <div className="network-row"><span className="network-icon">⌁</span><span>LAN</span><span className="separator">•</span><code>_cosync._udp.local:{payload.port}</code><button className="copy-button" title="Copy pairing payload" onClick={() => navigator.clipboard?.writeText(rawPayload)}>⧉</button></div>}
      <div className="security-note"><span>◇</span> End-to-end encrypted local pairing</div>
      {error && <p className="error-message">{error}</p>}
    </section>
  </main>;
}

function DeviceListScreen({ onPair }: { onPair: () => void }) {
  const [devices, setDevices] = useState<PairedDevice[]>([]);
  const [error, setError] = useState("");

  useEffect(() => {
    let active = true;
    const refresh = () => invoke<PairedDevice[]>("list_paired_devices").then((next) => active && setDevices(next)).catch((reason) => active && setError(String(reason)));
    refresh();
    const connectedListener = listen("paired-device-connected", refresh);
    const disconnectedListener = listen("paired-device-disconnected", refresh);
    return () => {
      active = false;
      void connectedListener.then((dispose) => dispose());
      void disconnectedListener.then((dispose) => dispose());
    };
  }, []);

  return <main className="devices-screen">
    <header className="window-heading"><div className="brand-mark"><PhoneIcon /></div><span>CoSync — paired devices</span><button className="text-button" onClick={onPair}>Pair another device</button></header>
    <section className="devices-content"><p className="eyebrow">TRUSTED DEVICES</p><h1>Paired devices</h1><p className="section-copy">Devices you trust stay on your local network and use pinned encryption keys.</p>
      <div className="device-list" role="list">{devices.length === 0 && <div className="empty-state">No devices paired yet.</div>}{devices.map((device) => <div className="device-row" role="listitem" key={device.device_id}><PhoneIcon /><div className="device-details"><strong>{device.device_name}</strong><span><StatusDot connected={device.connected} /> {device.connected ? "Connected" : "Offline"}</span></div><span className="device-address">{device.last_known_ip ?? "Local network"}</span></div>)}</div>
      <button className="primary-button" onClick={onPair}>Pair another device</button>{error && <p className="error-message">{error}</p>}
    </section>
  </main>;
}

function App() {
  const [view, setView] = useState<View>("pairing");
  useEffect(() => { const unlisten = listen<string>("navigate", (event) => { if (event.payload === "devices") setView("devices"); if (event.payload === "pairing") setView("pairing"); }); return () => { void unlisten.then((dispose) => dispose()); }; }, []);
  useEffect(() => { void getCurrentWindow().setTitle(view === "pairing" ? "CoSync — pair" : "CoSync — paired devices"); }, [view]);
  return view === "pairing" ? <PairingScreen onDevices={() => setView("devices")} /> : <DeviceListScreen onPair={() => setView("pairing")} />;
}

export default App;
