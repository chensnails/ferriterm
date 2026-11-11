import { describe, expect, it } from "vitest";

import { isRootPath, joinPath, parentPath } from "./paths";

describe("joinPath", () => {
  it("accepts both separators and collapses a trailing one", () => {
    expect(joinPath("/home", "notes.txt")).toBe("/home/notes.txt");
    expect(joinPath("/home/", "notes.txt")).toBe("/home/notes.txt");
    expect(joinPath("C:\\Users", "notes.txt")).toBe("C:\\Users/notes.txt");
  });

  it("anchors an empty base at the POSIX root", () => {
    expect(joinPath("", "notes.txt")).toBe("/notes.txt");
    expect(joinPath("/", "notes.txt")).toBe("/notes.txt");
  });
});

describe("parentPath", () => {
  it("climbs one level with either separator", () => {
    expect(parentPath("/home/user/notes.txt")).toBe("/home/user");
    expect(parentPath("C:\\Users\\me\\notes.txt")).toBe("C:\\Users\\me");
    expect(parentPath("C:/Users/me/notes.txt")).toBe("C:/Users/me");
  });

  it("stops at the root", () => {
    expect(parentPath("/")).toBe("/");
    expect(parentPath("/home")).toBe("/");
    expect(parentPath("C:/")).toBe("C:/");
    expect(parentPath("C:/Users")).toBe("C:/");
  });

  it("does not climb out of a UNC share", () => {
    expect(parentPath("\\\\server\\share")).toBe("\\\\server\\share");
    expect(parentPath("\\\\server\\share\\dir")).toBe("\\\\server\\share");
  });
});

describe("isRootPath", () => {
  it("recognises POSIX, drive and UNC roots", () => {
    expect(isRootPath("/")).toBe(true);
    expect(isRootPath("C:/")).toBe(true);
    expect(isRootPath("C:")).toBe(true);
    expect(isRootPath("\\\\server\\share")).toBe(true);
  });

  it("rejects ordinary paths", () => {
    expect(isRootPath("/home")).toBe(false);
    expect(isRootPath("C:/Users")).toBe(false);
    expect(isRootPath("\\\\server\\share\\dir")).toBe(false);
  });
});
