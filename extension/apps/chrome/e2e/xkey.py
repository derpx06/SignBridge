# Presses keys through the X server's XTEST extension (no xdotool needed). usage: xkey.py Alt_L+Shift_L+v
import ctypes, sys, time
x = ctypes.CDLL("libX11.so.6"); t = ctypes.CDLL("libXtst.so.6")
x.XOpenDisplay.restype = ctypes.c_void_p
d = x.XOpenDisplay(None)
if not d: sys.exit("no X display")
x.XKeysymToKeycode.argtypes = [ctypes.c_void_p, ctypes.c_ulong]
x.XStringToKeysym.argtypes = [ctypes.c_char_p]; x.XStringToKeysym.restype = ctypes.c_ulong
t.XTestFakeMotionEvent(ctypes.c_void_p(d), 0, 600, 400, 0); x.XFlush(ctypes.c_void_p(d)); time.sleep(0.2) # keys go to the window under the pointer when there is no window manager
codes = [x.XKeysymToKeycode(d, x.XStringToKeysym(k.encode())) for k in sys.argv[1].split("+")]
for c in codes: t.XTestFakeKeyEvent(ctypes.c_void_p(d), c, 1, 0); x.XFlush(ctypes.c_void_p(d)); time.sleep(0.05)
time.sleep(float(sys.argv[2]) if len(sys.argv) > 2 else 0.12)
for c in reversed(codes): t.XTestFakeKeyEvent(ctypes.c_void_p(d), c, 0, 0); x.XFlush(ctypes.c_void_p(d)); time.sleep(0.05)
