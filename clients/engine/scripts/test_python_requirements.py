#!/usr/bin/env python3

import pathlib
import re
import unittest


ENGINE_ROOT = pathlib.Path(__file__).resolve().parents[1]


class PythonRequirementsTest(unittest.TestCase):
    def test_every_requirement_has_an_exact_version(self):
        requirements = (ENGINE_ROOT / "requirements.txt").read_text(
            encoding="utf-8"
        )

        for line in requirements.splitlines():
            requirement = line.strip()
            if requirement and not requirement.startswith("#"):
                self.assertRegex(
                    requirement,
                    re.compile(r"^[A-Za-z0-9_.-]+==[A-Za-z0-9_.+!-]+$"),
                )


if __name__ == "__main__":
    unittest.main()
