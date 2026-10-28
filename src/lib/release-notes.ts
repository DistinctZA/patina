// Release notes come from commit messages: Markdown, with lines wrapped at
// a fixed width. The update panel is narrow, so join the wrapped lines back
// into whole sentences and show bullets as bullets.

export function plainNotes(md: string): string {
  const out: string[] = [];
  // Everything after "---" is the download page's install note.
  for (const raw of md.replace(/\n-{3,}[\s\S]*$/, "").split("\n")) {
    const line = raw.trim();
    const starts = line === "" || /^(#+|[-*])\s/.test(line);
    if (!starts && out.length && out[out.length - 1] !== "") {
      out[out.length - 1] += ` ${line}`;
    } else {
      out.push(line.replace(/^#+\s+/, "").replace(/^[-*]\s+/, "• "));
    }
  }
  return out
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}
