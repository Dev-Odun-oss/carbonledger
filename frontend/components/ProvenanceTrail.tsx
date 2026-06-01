"use client";

import { colors } from "../styles/design-system";

export interface ProvenanceEvent {
  type: "registered" | "verified" | "minted" | "listed" | "purchased" | "retired";
  label: string;
  timestamp: string;
  actor?: string;
  txHash?: string;
  detail?: string;
}

interface Props {
  events: ProvenanceEvent[];
}

const eventConfig: Record<ProvenanceEvent["type"], { icon: string; color: string }> = {
  registered: { icon: "📋", color: colors.neutral[500] },
  verified:   { icon: "✅", color: colors.primary[600] },
  minted:     { icon: "🌱", color: colors.primary[700] },
  listed:     { icon: "🏪", color: "#2563eb" },
  purchased:  { icon: "💼", color: "#7c3aed" },
  retired:    { icon: "🔒", color: "#dc2626" },
};

function truncateAddress(addr: string) {
  if (addr.length <= 16) return addr;
  return `${addr.slice(0, 6)}…${addr.slice(-6)}`;
}

export default function ProvenanceTrail({ events }: Props) {
  if (!events || events.length === 0) {
    return (
      <div style={{
        textAlign: "center", padding: "2.5rem 1rem",
        border: `1px dashed ${colors.neutral[300]}`,
        borderRadius: "0.75rem", color: colors.neutral[400],
      }}>
        <p style={{ fontSize: "2rem", margin: "0 0 0.5rem" }}>🔍</p>
        <p style={{ fontWeight: 600, color: colors.neutral[600], margin: "0 0 0.25rem" }}>No events found</p>
        <p style={{ fontSize: "0.875rem", margin: 0 }}>No on-chain events have been recorded for this credit yet.</p>
      </div>
    );
  }

  return (
    <>
      <style>{`
        @media (max-width: 480px) {
          .pt-event-row { flex-direction: column !important; gap: 0.25rem !important; }
          .pt-event-time { text-align: left !important; }
        }
      `}</style>
      <div style={{ position: "relative", paddingLeft: "2rem" }}>
        {/* Vertical line */}
        <div style={{
          position: "absolute", left: "0.6rem", top: 0, bottom: 0,
          width: "2px", background: colors.primary[200],
        }} />

        {events.map((event, i) => {
          const cfg = eventConfig[event.type];
          const isRetired = event.type === "retired";
          return (
            <div key={i} style={{ position: "relative", marginBottom: i < events.length - 1 ? "1.5rem" : 0 }}>
              {/* Dot */}
              <div style={{
                position: "absolute", left: "-1.65rem", top: "0.15rem",
                width: "1.25rem", height: "1.25rem",
                background: cfg.color, borderRadius: "50%",
                display: "flex", alignItems: "center", justifyContent: "center",
                fontSize: "0.6rem", border: "2px solid white",
                boxShadow: "0 0 0 2px " + cfg.color + "40",
              }}>
                {cfg.icon}
              </div>

              <div style={{
                background: isRetired ? "#fef2f2" : colors.surface,
                border: `1px solid ${isRetired ? "#fecaca" : colors.neutral[200]}`,
                borderRadius: "0.5rem",
                padding: "0.75rem 1rem",
              }}>
                <div className="pt-event-row" style={{ display: "flex", justifyContent: "space-between", alignItems: "center", gap: "0.5rem" }}>
                  <span style={{ fontWeight: 600, fontSize: "0.875rem", color: isRetired ? "#dc2626" : colors.neutral[800] }}>
                    {event.label}
                  </span>
                  <span className="pt-event-time" style={{ fontSize: "0.75rem", color: colors.neutral[400], whiteSpace: "nowrap" }}>
                    {new Date(event.timestamp).toLocaleDateString("en-US", {
                      year: "numeric", month: "short", day: "numeric",
                    })}
                  </span>
                </div>

                {event.actor && (
                  <p style={{ fontSize: "0.75rem", color: colors.neutral[500], margin: "0.25rem 0 0", fontFamily: "monospace" }}>
                    {truncateAddress(event.actor)}
                  </p>
                )}

                {event.detail && (
                  <p style={{ fontSize: "0.8rem", color: colors.neutral[600], margin: "0.25rem 0 0" }}>
                    {event.detail}
                  </p>
                )}

                {event.txHash && (
                  <a
                    href={`https://stellar.expert/explorer/testnet/tx/${event.txHash}`}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={{ fontSize: "0.7rem", color: colors.primary[600], fontFamily: "monospace", display: "inline-block", marginTop: "0.25rem" }}
                  >
                    {event.txHash.slice(0, 16)}… ↗
                  </a>
                )}
              </div>
            </div>
          );
        })}
      </div>
    </>
  );
}
