import "@testing-library/jest-dom/vitest";

// Mock scrollIntoView for jsdom environment
if (typeof Element !== "undefined") {
  Element.prototype.scrollIntoView = function () {};
}
