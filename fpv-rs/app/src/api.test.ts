import { describe, expect, it, vi } from "vitest";
import { bytes, clock, day, flightName, message, type FlightView } from "./api";

vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn(), convertFileSrc: (path: string) => path }));
vi.mock("@tauri-apps/api/event", () => ({ listen: vi.fn() }));

describe("clock", () => {
  it("writes m:ss, and m:ss.f with tenths", () => {
    expect(clock(0)).toBe("0:00");
    expect(clock(372.68)).toBe("6:12");
    expect(clock(61.04, true)).toBe("1:01.0");
    expect(clock(9.96, true)).toBe("0:10.0");
  });

  it("never shows a negative time", () => {
    expect(clock(-3)).toBe("0:00");
  });
});

describe("bytes", () => {
  it("writes a size the way a file manager does", () => {
    expect(bytes(0)).toBe("0 bytes");
    expect(bytes(999)).toBe("999 bytes");
    expect(bytes(340_000)).toBe("340 KB");
    expect(bytes(1_234_567_890)).toBe("1.2 GB");
    expect(bytes(12_400_000_000)).toBe("12 GB");
  });
});

describe("flightName", () => {
  const flight = { order: 3, title: null } as FlightView;

  it("falls back to the flight's number", () => {
    expect(flightName(flight)).toBe("Flight 3");
  });

  it("uses the pilot's title when there is one", () => {
    expect(flightName({ ...flight, title: "Rock gap" })).toBe("Rock gap");
  });
});

describe("message", () => {
  it("reads the strings Rust rejects with, Errors and anything else", () => {
    expect(message("not in the library: flight X")).toBe("not in the library: flight X");
    expect(message(new Error("boom"))).toBe("boom");
    expect(message(42)).toBe("42");
  });
});

describe("day", () => {
  it("has nothing to show for an undated session", () => {
    expect(day(null)).toBeNull();
  });
});
