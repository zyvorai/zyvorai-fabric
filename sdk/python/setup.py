# Copyright 2026 Zyvor AI Labs · https://zyvor.dev
# SPDX-License-Identifier: LicenseRef-Zyvor-Production-2.0
from setuptools import setup, find_packages

setup(
    name="fabricctl",
    version="0.1.0",
    packages=find_packages(),
    install_requires=["requests>=2.28"],
    python_requires=">=3.8",
    description="Python SDK for zyvor-fabricd",
    author="zyvor-fabricd contributors",
    license="LicenseRef-Zyvor-Production-2.0",
    entry_points={
        "console_scripts": [
            "fabricctl=fabricctl.cli:main",
        ],
    },
)
