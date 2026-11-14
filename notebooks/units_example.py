"""Presentation for the mixed-unit report; all conversion work stays in SQL."""

import matplotlib.pyplot as plt
import numpy as np
from vgi_notebooks import finish

COLORS = {"Cedar": "#27745B", "Harbor": "#437EB4", "Mesa": "#BD7837"}
SOURCE = "Authored sampling round · VGI Unit Conversion v0.4.0 · Synthetic measurements, no elapsed time"


def plot_mechanical(mechanical):
    fig, axes = plt.subplots(1, 2, figsize=(11, 5.6))
    for ax, metric, label in zip(
        axes, ["pressure", "sample_volume"], ["Pressure (kPa)", "Sample volume (L)"]
    ):
        rows = mechanical[mechanical.metric == metric].sort_values("site")
        positions = np.arange(len(rows))
        ax.barh(positions, rows.report_value, color=[COLORS[s] for s in rows.site], height=0.58)
        ax.set_yticks(positions, rows.site)
        ax.invert_yaxis()
        ax.set(xlabel=label, ylabel="", xlim=(0, rows.report_value.max() * 1.42))
        ax.grid(axis="y", visible=False)
        for position, row in zip(positions, rows.itertuples()):
            ax.text(
                row.report_value + rows.report_value.max() * 0.025,
                position,
                f"{row.report_value:.2f} {row.report_unit}",
                va="center",
                fontsize=10,
            )
    finish(
        fig,
        "Mixed labels become comparable within each dimension",
        "One synthetic sampling round · independent x-scales for pressure and sampled volume",
        SOURCE,
        name="normalized-measurements",
    )


def plot_thermal(thermal, REPORT_TEMP_UNIT):
    fig, ax = plt.subplots(figsize=(11, 5.4))
    lo = min(thermal.inlet_report.min(), thermal.outlet_report.min())
    hi = max(thermal.inlet_report.max(), thermal.outlet_report.max())
    span = max(hi - lo, 1)
    for y, row in enumerate(thermal.itertuples()):
        color = COLORS[row.site]
        ax.plot([row.inlet_report, row.outlet_report], [y, y], color=color, lw=4, alpha=0.65)
        ax.scatter(
            row.inlet_report,
            y,
            s=90,
            facecolors="white",
            edgecolors=color,
            linewidth=2,
            zorder=3,
            label="Inlet" if y == 0 else None,
        )
        ax.scatter(
            row.outlet_report,
            y,
            s=90,
            color=color,
            marker="s",
            zorder=3,
            label="Outlet" if y == 0 else None,
        )
        ax.text(
            max(row.inlet_report, row.outlet_report) + span * 0.05,
            y,
            f"Rise {row.rise_report:.1f} Δ{REPORT_TEMP_UNIT}",
            va="center",
            color=color,
        )
    ax.set_yticks(range(len(thermal)), thermal.site)
    ax.set(
        xlabel=f"Absolute temperature ({REPORT_TEMP_UNIT})",
        ylabel="",
        xlim=(lo - span * 0.1, hi + span * 0.5),
        ylim=(-0.55, len(thermal) - 0.45),
    )
    ax.invert_yaxis()
    fig.legend(*ax.get_legend_handles_labels(), loc="upper center", bbox_to_anchor=(0.5, 0.84), ncol=2)
    finish(
        fig,
        "Compare endpoints, then compute the temperature rise",
        f"Absolute readings in {REPORT_TEMP_UNIT} · changes labelled Δ{REPORT_TEMP_UNIT}",
        SOURCE,
        name=f"temperature-pairs-{REPORT_TEMP_UNIT}",
    )


def plot_difference(curve, unit):
    fig, axes = plt.subplots(1, 2, figsize=(12, 6))
    for ax, field, title, color in zip(
        axes,
        ["correct_difference", "absolute_misapplied"],
        ["Offset cancelled", "Absolute function misapplied"],
        ["#27745B", "#B26043"],
    ):
        values = curve[field]
        ax.plot(curve.change_c, values, color=color, lw=2.8)
        probe = curve.loc[curve.change_c == 10, field].iloc[0]
        ax.scatter([10], [probe], color=color, s=60, zorder=4)
        ax.annotate(
            f"{probe:.2f} Δ{unit}",
            (10, probe),
            xytext=(10, -22),
            va="top",
            textcoords="offset points",
            color=color,
        )
        ax.set_title(title, loc="left", fontsize=12, pad=12)
        ax.set_xlabel("Hypothetical temperature change (Δ°C)")
        ax.set_ylabel(f"Reported change (Δ{unit})")
        span = max(float(values.max() - values.min()), 1.0)
        ax.set_ylim(float(values.min()) - 0.15 * span, float(values.max()) + 0.25 * span)
    finish(
        fig,
        "Difference conversion removes the absolute offset",
        "Independent vertical ranges · markers show a 10 Δ°C input · right panel demonstrates the mistake",
        "Authored changes from −10 to +25 Δ°C · VGI Unit Conversion v0.4.0",
        name=f"difference-offset-{unit}",
    )
