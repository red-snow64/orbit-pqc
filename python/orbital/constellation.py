import numpy as np
from skyfield.api import EarthSatellite, load, wgs84
from skyfield.positionlib import ICRF
from typing import List, Tuple, Dict

SPEED_OF_LIGHT_KM_S = 299792.458
EARTH_RADIUS_KM = 6378.137
ATMOSPHERE_BUFFER_KM = 100.0  # Atmospheric tangent height cutoff

class WalkerDeltaConstellation:
    def __init__(self, total_sats: int = 24, planes: int = 4, f_phase: int = 1, altitude_km: float = 550.0, inc_deg: float = 53.0):
        self.total_sats = total_sats
        self.planes = planes
        self.sats_per_plane = total_sats // planes
        self.altitude_km = altitude_km
        self.inc_deg = inc_deg
        self.ts = load.timescale()
        self.satellites = self._generate_constellation()

    def _generate_constellation(self) -> Dict[str, EarthSatellite]:
        """Generates synthetic EarthSatellite instances using standard Keplerian elements."""
        sats = {}
        mean_motion_rev_per_day = 86400.0 / (2.0 * np.pi * np.sqrt(((EARTH_RADIUS_KM + self.altitude_km) ** 3) / 398600.4418))
        epoch = self.ts.utc(2026, 1, 1, 0, 0, 0)

        sat_idx = 0
        for p in range(self.planes):
            raan_deg = p * (360.0 / self.planes)
            for s in range(self.sats_per_plane):
                mean_anomaly_deg = s * (360.0 / self.sats_per_plane) + (p * 360.0 / self.total_sats)

                # Synthetic TLE formulation
                line1 = f"1 {sat_idx+1:05d}U 26001A   26001.00000000  .00000000  00000-0  00000-0 0  9999"
                line2 = f"2 {sat_idx+1:05d} {self.inc_deg:8.4f} {raan_deg:8.4f} 0001000   0.0000 {mean_anomaly_deg:8.4f} {mean_motion_rev_per_day:11.8f}00001"

                sat_name = f"SAT_P{p+1}_N{s+1}"
                sats[sat_name] = EarthSatellite(line1, line2, sat_name, self.ts)
                sat_idx += 1
        return sats

    def compute_isl_trajectory(self, sat_a_name: str, sat_b_name: str, duration_sec: int, dt_sec: float = 0.5) -> List[Dict]:
        """
        Propagates orbits across duration_sec and outputs dynamic geometry:
        Distance (km), Line-of-Sight condition, and One-way Propagation Delay (ms).
        """
        sat_a = self.satellites[sat_a_name]
        sat_b = self.satellites[sat_b_name]

        t_base = self.ts.utc(2026, 1, 1, 12, 0, 0)
        times = self.ts.utc(2026, 1, 1, 12, 0, np.arange(0, duration_sec, dt_sec))

        pos_a = sat_a.at(times).position.km
        pos_b = sat_b.at(times).position.km

        trajectory = []
        effective_earth_r = EARTH_RADIUS_KM + ATMOSPHERE_BUFFER_KM

        for idx, t_offset in enumerate(np.arange(0, duration_sec, dt_sec)):
            r_a = pos_a[:, idx]
            r_b = pos_b[:, idx]

            # Vector separation and Euclidean distance
            r_ab = r_b - r_a
            distance_km = float(np.linalg.norm(r_ab))

            # Geometric Earth-occlusion line segment test
            # Shortest distance from Earth center (0,0,0) to line segment r_a -> r_b
            d_vec = r_ab / distance_km
            t_proj = -np.dot(r_a, d_vec)

            if t_proj < 0:
                closest_dist = np.linalg.norm(r_a)
            elif t_proj > distance_km:
                closest_dist = np.linalg.norm(r_b)
            else:
                closest_dist = np.linalg.norm(r_a + t_proj * d_vec)

            has_los = bool(closest_dist > effective_earth_r)
            delay_ms = (distance_km / SPEED_OF_LIGHT_KM_S) * 1000.0 if has_los else None

            trajectory.append({
                "time_offset_s": round(float(t_offset), 2),
                "distance_km": round(distance_km, 3),
                "has_los": has_los,
                "prop_delay_ms": round(delay_ms, 3) if delay_ms else None,
                "closest_approach_km": round(float(closest_dist), 2)
            })

        return trajectory