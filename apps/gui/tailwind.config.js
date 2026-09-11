export default {
  content: ["./index.html", "./src/**/*.{js,ts,jsx,tsx}"],
  theme: {
    extend: {
      colors: {
        "bg-primary": "#1a1a2e",
        "bg-secondary": "#16213e",
        "bg-tertiary": "#0f3460",
        "accent-red": "#e94560",
        "accent-blue": "#0f3460",
      },
      fontFamily: {
        mono: [
          "JetBrains Mono",
          "Fira Code",
          "Consolas",
          "Monaco",
          "monospace",
        ],
      },
    },
  },
  plugins: [],
};
