import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from rife_plan import rife_plan


class RifePlanTests(unittest.TestCase):
    def test_24(self):
        p = rife_plan(23.976)
        self.assertEqual(p.kind, "multiply_then_thin")
        self.assertEqual((p.multi, p.cycle), (5, 2))

    def test_30(self):
        p = rife_plan(29.97)
        self.assertEqual(p.kind, "multiply")
        self.assertEqual(p.multi, 2)

    def test_60_skip(self):
        p = rife_plan(60.0)
        self.assertEqual(p.kind, "skip")


if __name__ == "__main__":
    unittest.main()
