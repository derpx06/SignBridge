// What the browser tells us about the microphone, kept live: permission, devices, and a level meter for tests.
import { useCallback, useEffect, useState } from "react";
import type { MicDevice, MicPermission, MicProbe } from "@voicio/core";

export async function probeMic(): Promise<MicProbe> {
  let permission: MicPermission = "unknown";
  try { permission = (await navigator.permissions.query({ name: "microphone" as PermissionName })).state as MicPermission; } catch { /* older Chrome */ }
  let devices: MicDevice[] = [];
  try {
    const all = (await navigator.mediaDevices.enumerateDevices()).filter((d) => d.kind === "audioinput");
    devices = all.map((d, i) => ({ id: d.deviceId, label: d.label || `Microphone ${i + 1}` }));
  } catch { /* no media devices API */ }
  return { permission, devices };
}

/** Permission and device list that update when a microphone is plugged in, removed, allowed or blocked. */
export function useMicProbe(): MicProbe & { ready: boolean; refresh: () => void } {
  const [probe, setProbe] = useState<MicProbe>({ permission: "unknown", devices: [] });
  const [ready, setReady] = useState(false);
  const refresh = useCallback(() => { void probeMic().then((p) => { setProbe(p); setReady(true); }); }, []);
  useEffect(() => {
    refresh();
    let status: PermissionStatus | undefined;
    void navigator.permissions.query({ name: "microphone" as PermissionName }).then((s) => { status = s; s.onchange = refresh; }).catch(() => {});
    navigator.mediaDevices?.addEventListener?.("devicechange", refresh);
    return () => { if (status) status.onchange = null; navigator.mediaDevices?.removeEventListener?.("devicechange", refresh); };
  }, [refresh]);
  return { ...probe, ready, refresh };
}

/** Chrome's page for this extension's site settings, where Microphone can be set back to Allow or Ask. */
export const micSettingsUrl = () => `chrome://settings/content/siteDetails?site=${encodeURIComponent(`chrome-extension://${chrome.runtime.id}`)}`;

/** What dictation needs from the microphone. Chrome's automatic gain control is off on purpose: on Linux it can drive the
 * system input volume down, which makes a working microphone deliver silence. */
export const micAudio = (deviceId = ""): MediaTrackConstraints => ({
  channelCount: 1, echoCancellation: false, noiseSuppression: true, autoGainControl: false, ...(deviceId ? { deviceId: { exact: deviceId } } : {}),
});

export interface Meter { stop: () => void; label: string }

/** Speech is clearly above a quiet room (about -48 dB). */
export const HEARD_DB = -40;

/** Opens a microphone and reports its level (0..1) about 30 times a second. Throws a DOMException when it cannot. */
export async function startMeter(deviceId: string, onLevel: (v: number, rawDb: number) => void): Promise<Meter> {
  const stream = await navigator.mediaDevices.getUserMedia({ audio: micAudio(deviceId) });
  const ctx = new AudioContext();
  void ctx.resume().catch(() => {}); // a suspended context reads as silence (never awaited: it can stay pending)
  const an = ctx.createAnalyser();
  an.fftSize = 512;
  ctx.createMediaStreamSource(stream).connect(an);
  const buf = new Uint8Array(an.fftSize);
  let raf = 0, stopped = false;
  const loop = () => {
    if (stopped) return;
    an.getByteTimeDomainData(buf);
    let sum = 0;
    for (const b of buf) { const x = (b - 128) / 128; sum += x * x; }
    const rms = Math.sqrt(sum / buf.length);
    const db = 20 * Math.log10(rms + 1e-6);
    onLevel(Math.max(0, Math.min(1, (db + 60) / 45)), db); // same scale as the recorder: -60 dB is empty, -15 dB is full
    raf = requestAnimationFrame(loop);
  };
  loop();
  return { label: stream.getAudioTracks()[0]?.label ?? "", stop: () => { stopped = true; cancelAnimationFrame(raf); stream.getTracks().forEach((t) => t.stop()); void ctx.close(); } };
}
