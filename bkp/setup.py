from setuptools import setup, find_packages

setup(
    name="jev-orchestrator",
    version="0.1.0",
    author="Jev AI Team",
    author_email="team@jev.ai",
    description="Terminal-based AI development environment with intelligent orchestration",
    long_description=open("README.md").read(),
    long_description_content_type="text/markdown",
    url="https://github.com/jev-ai/jev-orchestrator",
    packages=find_packages(),
    classifiers=[
        "Programming Language :: Python :: 3",
        "License :: OSI Approved :: MIT License",
        "Operating System :: OS Independent",
    ],
    python_requires=">=3.8",
    install_requires=[
        "openai>=1.0.0",
        "anthropic>=0.3.0",
    ],
    entry_points={
        "console_scripts": [
            "jev=apps.cli.main:main",
        ],
    },
    extras_require={
        "dev": [
            "pytest>=7.0.0",
            "black>=23.0.0",
            "flake8>=6.0.0",
        ]
    }
)