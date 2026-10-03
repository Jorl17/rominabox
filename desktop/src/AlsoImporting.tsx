// The files we copy with the game, after the game file itself: its
// companions and the patches we apply to it in the export.

// We name three tracks. Six is more than a handful, so we show a count.
const NAMED_COMPANIONS = 5;

function alsoImporting(files: string[]): string | null {
  if (files.length < 2) return null;
  const extras = files.slice(1);
  if (extras.length > NAMED_COMPANIONS) {
    return `Also importing ${extras.length} files`;
  }
  const dot = files[0].lastIndexOf(".");
  const stem = dot > 0 ? files[0].slice(0, dot) : files[0];
  const named = extras.map((name) => {
    if (!stem || !name.startsWith(stem)) return name;
    const rest = name.slice(stem.length).trim();
    return rest || name;
  });
  return `Also importing: ${named.join(", ")}`;
}

export function AlsoImporting({ files }: { files: string[] }) {
  const also = alsoImporting(files);
  if (!also) return null;
  return <p className="traveling-also">{also}</p>;
}
