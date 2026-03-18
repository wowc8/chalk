import { motion } from "framer-motion";
import { useState, useEffect } from "react";

const ONOMATOPOEIAS = ["BAM!", "ZERP!", "BEEP-BOOP!", "DIGEST!", "ZAP!", "WHIRR!"];

export function BatmanOverlay() {
  const [words, setWords] = useState<
    { text: string; x: number; y: number; rotation: number; id: number }[]
  >([]);

  useEffect(() => {
    let counter = 0;
    const interval = setInterval(() => {
      const text =
        ONOMATOPOEIAS[Math.floor(Math.random() * ONOMATOPOEIAS.length)];
      setWords((prev) => [
        ...prev.slice(-4),
        {
          text,
          x: 10 + Math.random() * 80,
          y: 10 + Math.random() * 80,
          rotation: -15 + Math.random() * 30,
          id: counter++,
        },
      ]);
    }, 800);

    return () => clearInterval(interval);
  }, []);

  return (
    <motion.div
      initial={{ opacity: 0 }}
      animate={{ opacity: 1 }}
      exit={{ opacity: 0 }}
      className="fixed inset-0 z-50 flex items-center justify-center"
      style={{ backdropFilter: "blur(8px)" }}
    >
      {/* Frosted glass */}
      <div className="absolute inset-0 bg-bat-glass" />

      {/* Floating onomatopoeias */}
      {words.map((w) => (
        <motion.span
          key={w.id}
          initial={{ opacity: 0, scale: 0.3 }}
          animate={{ opacity: [0, 1, 1, 0], scale: [0.3, 1.2, 1, 0.8], y: [0, -20, -30, -50] }}
          transition={{ duration: 2, ease: "easeOut" }}
          className="absolute text-3xl font-black tracking-wider select-none pointer-events-none"
          style={{
            left: `${w.x}%`,
            top: `${w.y}%`,
            transform: `rotate(${w.rotation}deg)`,
            color: w.id % 2 === 0 ? "#f0c040" : "#00d4ff",
            textShadow: "0 0 20px currentColor, 0 0 40px currentColor",
            fontFamily: "'Impact', 'Arial Black', sans-serif",
          }}
        >
          {w.text}
        </motion.span>
      ))}

      {/* Central spinner */}
      <div className="relative z-10 text-center">
        <motion.div
          animate={{ rotate: 360 }}
          transition={{ duration: 2, repeat: Infinity, ease: "linear" }}
          className="w-16 h-16 mx-auto mb-4 border-4 border-bat-cyan border-t-transparent rounded-full"
        />
        <p className="text-bat-gold text-lg font-bold tracking-widest uppercase">
          Processing...
        </p>
      </div>
    </motion.div>
  );
}
