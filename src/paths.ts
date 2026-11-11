/**
 * Path helpers shared by the file pane and the transfer panel.
 *
 * The local side hands us Windows paths (`C:\Users\me`) while the remote side
 * hands us POSIX ones, so every join and parent lookup must accept both
 * separators. Treating only `/` as a separator moved renamed files to the
 * drive root, which is exactly the bug this module prevents.
 */

export function joinPath(dir: string, name: string): string {
  if (!dir) return `/${name}`;
  return `${dir.replace(/[\\/]+$/, "")}/${name}`;
}

function isUncRoot(path: string): boolean {
  // `\\server\share` is a share root; climbing out would leave the share.
  return /^[\\/]{2}[^\\/]+[\\/]+[^\\/]+[\\/]*$/.test(path);
}

export function parentPath(path: string): string {
  const trimmed = path.replace(/[\\/]+$/, "");
  if (isUncRoot(trimmed)) return trimmed;
  const index = Math.max(trimmed.lastIndexOf("/"), trimmed.lastIndexOf("\\"));
  if (index <= 0) return path.startsWith("/") ? "/" : trimmed;
  const parent = trimmed.slice(0, index);
  // `C:` on its own is not a directory; the root is `C:/`.
  return /^[A-Za-z]:$/.test(parent) ? `${parent}/` : parent;
}

export function isRootPath(path: string): boolean {
  return (
    /^[\\/]+$/.test(path) ||
    /^[A-Za-z]:[\\/]*$/.test(path) ||
    isUncRoot(path)
  );
}
