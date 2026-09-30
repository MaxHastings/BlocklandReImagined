"""Usage: python tools/tge_tank_turning.py

2D model of Torque's WheeledVehicle::updateForces for v20's Tank on flat
ground (static wheel loads), for comparing turning with our Rapier wheels.
Torque axes: x right, y forward. Values: Vehicle_Tank.cs (datablock, tankTire)
and TankVehicle::onAdd (4-wheel steering 1,1,-0.8,-0.8; all powered)."""
import math, sys

MASS = 300.0
G = 20.0
DT = 0.032 / 4  # integration = 4
ENGINE_TORQUE = 25000.0
MAX_WHEEL_SPEED = 20.0
MAX_STEER = 0.9785
DRAG = 1.6
RADIUS = 0.66
LAT_F, LAT_D, LAT_R = 18000.0, 4000.0, 0.01
LON_F, LON_D, LON_R = 14000.0, 2000.0, 0.01
MU = 5.0
# Hub positions (x right, y forward): front at y=+1.6, rear y=-1.55.
WHEELS = [(-1.92, 1.6, 1.0), (1.92, 1.6, 1.0), (-1.92, -1.55, -0.8), (1.92, -1.55, -0.8)]
# Shape bounds box inertia (massBox commented out in the datablock).
BX, BY = 4.87, 6.47
IZ = MASS / 12.0 * (BX * BX + BY * BY)


def run(steer, seconds=6.0, throttle=1.0):
    x = y = heading = 0.0  # heading: angle of forward (+y) from world +y, left positive
    vx = vy = 0.0          # world velocity
    w = 0.0                # yaw rate, left positive
    avel = [0.0] * 4
    Dx = [0.0] * 4
    Dy = [0.0] * 4
    slipping = [False] * 4
    load = MASS * G / 4
    amom = MASS / 4
    q = -(steer * abs(steer))
    s, c = math.sin(q), math.cos(q)
    t = 0.0
    while t < seconds:
        ch, sh = math.cos(heading), math.sin(heading)
        bx = (ch, -sh)   # body right in world (heading left-positive)
        by = (sh, ch)    # body forward in world
        fx = fy = torque = 0.0
        for i, (px, py, k) in enumerate(WHEELS):
            # wheel axle direction: bx*cos + by*sin*k
            ax = (bx[0] * c + by[0] * s * k, bx[1] * c + by[1] * s * k)
            n = math.hypot(*ax)
            tireX = (ax[0] / n, ax[1] / n)
            tireY = (-tireX[1], tireX[0])  # forward = rotate right by +90 (normal up)
            # contact point world offset
            rx = bx[0] * px + by[0] * py
            ry = bx[1] * px + by[1] * py
            # velocity at contact: v + w x r (w about up, left positive)
            cvx = vx - w * ry
            cvy = vy + w * rx
            xv = tireX[0] * cvx + tireX[1] * cvy
            yv = tireY[0] * cvx + tireY[1] * cvy
            ddy = (avel[i] * RADIUS - yv) - LON_R * abs(avel[i]) * Dy[i]
            Dy[i] += ddy * DT
            Fy = LON_F * Dy[i] + LON_D * ddy
            ddx = xv - LAT_R * abs(avel[i]) * Dx[i]
            Dx[i] += ddx * DT
            Fx = -(LAT_F * Dx[i] + LAT_D * ddx)
            mu = MU
            Fn = (load * mu) ** 2
            Fw = Fx * Fx + Fy * Fy
            if Fw > Fn:
                K = math.sqrt(Fn / Fw)
                Fy *= K; Fx *= K; Dy[i] *= K; Dx[i] *= K
                slipping[i] = True
            else:
                slipping[i] = False
            gx = tireX[0] * Fx + tireY[0] * Fy
            gy = tireX[1] * Fx + tireY[1] * Fy
            fx += gx; fy += gy
            torque += rx * gy - ry * gx
            max_avel = MAX_WHEEL_SPEED / RADIUS
            scale = 0.0 if abs(avel[i]) > max_avel else 1 - abs(avel[i]) / max_avel
            avel[i] += ((scale * ENGINE_TORQUE * throttle) - Fy * RADIUS) / amom * DT
        fx -= vx * DRAG
        fy -= vy * DRAG
        torque -= w * IZ * DRAG  # angMomentum * drag
        vx += fx / MASS * DT
        vy += fy / MASS * DT
        w += torque / IZ * DT
        x += vx * DT; y += vy * DT; heading += w * DT
        t += DT
    speed = math.hypot(vx, vy)
    return speed, w, (speed / abs(w) if abs(w) > 1e-6 else float('inf'))


if __name__ == "__main__":
    for steer in [0.0, 0.25, 0.5, 0.75, MAX_STEER]:
        sp, w, r = run(steer)
        print(f"steer {steer:.3f}: speed {sp:.2f}  yaw {w:+.3f} rad/s  radius {r:.2f}")
