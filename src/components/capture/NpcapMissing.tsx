// Shown instead of the packet list when Npcap is missing or locked to administrators.

import type { CaptureSupport } from "../../lib/types";
import { RecordIcon } from "../icons";

interface Props {
  support: CaptureSupport;
  checking: boolean;
  onGet(): void;
  onCheck(): void;
}

export function NpcapMissing({ support, checking, onGet, onCheck }: Props) {
  const adminOnly = /admin/i.test(support.problem ?? "");

  return (
    <section className="panel capture-card step-in" aria-label="Npcap is needed">
      <span className="capture-card-icon" aria-hidden="true">
        <RecordIcon />
      </span>
      {adminOnly ? (
        <>
          <h2>Npcap only lets administrators capture</h2>
          <p>
            Npcap, the capture driver Portlight uses, is installed, but it was set up with the option{" "}
            <strong>"Restrict Npcap driver's access to Administrators only"</strong>. Portlight runs as you, not as an
            administrator, so Npcap will not hand it any packets.
          </p>
          <p>
            To fix it, run the Npcap installer again and leave that box <strong>unticked</strong>. Then come back and press
            Check again.
          </p>
        </>
      ) : (
        <>
          <h2>Packet capture needs Npcap</h2>
          <p>
            <strong>Npcap</strong> is the free capture driver Wireshark also uses. It lets programs see the packets passing
            through your network cards. Its licence does not allow Portlight to include it, so it is a separate, one-time
            install.
          </p>
          <p>
            Get it from npcap.com, run the installer with the default options, then come back and press Check again. The
            rest of Portlight works without it.
          </p>
        </>
      )}
      {support.problem && <p className="capture-card-note">Portlight says: {support.problem}</p>}
      <div className="capture-card-actions">
        <button className="button button-solid" onClick={onGet}>
          {adminOnly ? "Get the Npcap installer" : "Get Npcap"}
        </button>
        <button className="button" disabled={checking} onClick={onCheck}>
          {checking ? "Checking…" : "Check again"}
        </button>
      </div>
    </section>
  );
}
