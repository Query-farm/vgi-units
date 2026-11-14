"""Small presentation helpers for the units walkthrough; domain work stays in SQL."""

from __future__ import annotations


import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns
from IPython.display import Markdown, display

COLORS = {"Cedar": "#27745B", "Harbor": "#437EB4", "Mesa": "#BD7837"}
SOURCE = "Source: synthetic sampling round · Rust vgi-units v0.4.0 worker through Haybarn / VGI"


def sql_text(value: str) -> str:
    """Quote one SQL string, including shell quoting for worker commands."""
    return "'" + value.replace("'", "''") + "'"


def prose(value: str) -> None:
    """Keep result-derived prose visible while folding its Python source."""
    display(Markdown(value))


def set_theme() -> None:
    sns.set_theme(
        style="whitegrid",
        context="notebook",
        palette=list(COLORS.values()),
        rc={
            "figure.dpi": 150,
            "figure.facecolor": "white",
            "axes.facecolor": "white",
            "axes.edgecolor": "#CCD5CF",
            "axes.labelcolor": "#405047",
            "text.color": "#243B2F",
            "xtick.color": "#58685F",
            "ytick.color": "#58685F",
            "grid.color": "#E8EDE9",
            "axes.spines.top": False,
            "axes.spines.right": False,
            "axes.titlelocation": "left",
            "axes.titleweight": "semibold",
            "legend.frameon": False,
            "lines.linewidth": 2,
            "text.parse_math": False,
        },
    )
    pd.set_option("display.max_columns", 15)
    pd.set_option("display.precision", 5)


def chart(fig, title: str, subtitle: str, source: str = SOURCE) -> None:
    """Give every chart a title, context and local provenance."""
    fig.text(0.08, 0.99, title, ha="left", va="top", fontsize=17, fontweight="semibold")
    fig.text(0.08, 0.91, subtitle, ha="left", va="top", fontsize=10.5, color="#58685F")
    sns.despine(fig=fig)
    fig.tight_layout(rect=(0, 0.09, 1, 0.83), w_pad=3)
    fig.text(0.08, 0.02, source, ha="left", va="bottom", fontsize=8.5, color="#78857D")
    plt.show()
