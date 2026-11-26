import "./admin-tag.css";

/** The server's admin tag beside a player's name (social-identity step 5).
 * It comes from the match server's relay, set by deploy configuration. */
export function AdminTag({ compact = false }: { compact?: boolean }) {
  return (
    <span className={`admin-tag${compact ? " compact" : ""}`} title="Admin: runs this server">
      Admin
    </span>
  );
}
