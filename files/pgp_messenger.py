#!/usr/bin/env python3
"""PGP Messenger — modern dark messenger layout, all in-app panels."""

import warnings
warnings.filterwarnings("ignore", category=DeprecationWarning)

import tkinter as tk
from tkinter import messagebox, filedialog
import json, os, sys, re, datetime, threading, subprocess, shutil

import pgpy
from pgpy.constants import (
    PubKeyAlgorithm, KeyFlags, HashAlgorithm,
    SymmetricKeyAlgorithm, CompressionAlgorithm,
)

try:
    from PIL import Image, ImageDraw, ImageTk
    _HAS_PIL = True
except Exception:
    _HAS_PIL = False

# Windows gets custom borderless chrome (rounded corners, custom title bar via
# ctypes/DWM); macOS and Linux fall back to a normal native-decorated window.
IS_WIN = (os.name == "nt")

_INSTANCE_MUTEX = None
def _acquire_single_instance():
    """True if we're the only instance; False if PGPM is already running.
    Uses a named Windows mutex (auto-released when the process exits) so the user
    can't stack up multiple windows by double-clicking the exe repeatedly."""
    if not IS_WIN:
        return True
    try:
        import ctypes
        global _INSTANCE_MUTEX
        k32 = ctypes.windll.kernel32
        _INSTANCE_MUTEX = k32.CreateMutexW(None, False, "PGPM_single_instance_mutex")
        return k32.GetLastError() != 183           # 183 = ERROR_ALREADY_EXISTS
    except Exception:
        return True


def _resource(name):
    """Path to a bundled resource — works under PyInstaller (sys._MEIPASS) too.
    Icons live in an `assets/` subfolder. Check the script dir and its parent
    (assets/ + fonts/ sit at the repo root while the source lives in files/),
    trying the assets/ subdir first, so it resolves both frozen and from source."""
    base = getattr(sys, "_MEIPASS", os.path.dirname(os.path.abspath(__file__)))
    for root in (base, os.path.dirname(base)):
        for cand in (os.path.join(root, "assets", name), os.path.join(root, name)):
            if os.path.exists(cand):
                return cand
    return os.path.join(base, name)


def _register_fonts():
    """Load bundled Inter fonts into this process (Windows) so Tk can use them
    without a system install. Returns True if at least one face registered."""
    if not IS_WIN:
        return False
    added = 0
    try:
        import ctypes
        FR_PRIVATE = 0x10
        gdi = ctypes.windll.gdi32
        gdi.AddFontResourceExW.argtypes = [ctypes.c_wchar_p, ctypes.c_uint, ctypes.c_void_p]
        for fn in ("Inter-Regular.ttf", "Inter-Medium.ttf",
                   "Inter-SemiBold.ttf", "Inter-Bold.ttf"):
            p = _resource(os.path.join("fonts", fn))
            if os.path.exists(p):
                added += gdi.AddFontResourceExW(p, FR_PRIVATE, None)
    except Exception:
        return False
    return added > 0

# ─── Palette ──────────────────────────────────────────────────────
BK    = "#0d0d0d"   # main bg
SB    = "#111111"   # contact row color
SBG   = "#0E0E0E"   # contact-list / sidebar panel background
HDR   = "#111111"   # headers
BUB_S = "#1f2c47"   # message bubble (same for both)
BUB_R = "#1f2c47"   # message bubble (same for both)
INP   = "#1a1a1a"   # inputs
DIV   = "#2a2a2a"   # dividers
HOV   = "#191919"   # hover
SCRL  = "#3a3a3a"   # scrollbar thumb
TBR   = "#141414"   # title bar
TBR   = "#141414"   # title bar
T1    = "#e0e0e0"   # primary text
T2    = "#9e9e9e"   # secondary
T3    = "#606060"   # muted
T4    = "#404040"   # very muted
GN    = "#3dff7a"   # green
RD    = "#ff4444"   # red
AM    = "#e0a030"   # amber (expiring soon)
WT    = "#ffffff"   # white (active-key border)

UI  = "Microsoft JhengHei"                           # per request
MON = "Consolas"
FM   = (UI, 10)
FB   = (UI, 10, "bold")
FS   = (UI, 9)
FMO  = (MON, 9)
FBUB = (UI, 9, "bold")

_DATA     = os.path.join(os.path.expanduser("~"), ".pgp_messenger")
_CONTACTS = os.path.join(_DATA, "contacts.json")
_KEYS     = os.path.join(_DATA, "keys.json")
_NOTES    = os.path.join(_DATA, "notes.txt")
_CONFIG   = os.path.join(_DATA, "config.json")   # remembers the active key fingerprint
os.makedirs(_DATA, exist_ok=True)

# Custom (user-supplied) icons: copy from the Desktop into the data dir once, so
# they survive if the Desktop file is later moved/removed. (icon kind -> dst path)
_DESKTOP = os.path.join(os.path.expanduser("~"), "Desktop")
_CUSTOM_PNG = {
    "key":     (os.path.join(_DATA, "key.png"),     os.path.join(_DESKTOP, "key-icon-1.png")),
    "keycard": (os.path.join(_DATA, "keycard.png"), os.path.join(_DESKTOP, "sfbsb.png")),
    "encrypt": (os.path.join(_DATA, "encrypt.png"), os.path.join(_DESKTOP, "encrypt.png")),
    "verify":  (os.path.join(_DATA, "verify.png"),  os.path.join(_DESKTOP, "sign1.png")),
    "plus":    (os.path.join(_DATA, "plus.png"),    os.path.join(_DESKTOP, "uygfd78.png")),
    "shuffle": (os.path.join(_DATA, "shuffle.png"), os.path.join(_DESKTOP, "17446640.png")),
    "back":    (os.path.join(_DATA, "back.png"),    os.path.join(_DESKTOP, "128x128", "sdfd.png")),
}
for _dst, _src in _CUSTOM_PNG.values():
    try:
        if os.path.exists(_src):           # always refresh from the latest Desktop file
            shutil.copyfile(_src, _dst)
    except Exception:
        pass

# App window/taskbar/dock icon: refresh from the Desktop copy if present.
_ICON_DATA = os.path.join(_DATA, "icon.png")
try:
    _icon_src = os.path.join(_DESKTOP, "icon.png")
    if os.path.exists(_icon_src):
        shutil.copyfile(_icon_src, _ICON_DATA)
except Exception:
    pass


def _icon_path():
    """Resolve the app icon: bundled copy first, then the Desktop-synced copy."""
    for p in (_resource("icon.png"), _ICON_DATA):
        if p and os.path.exists(p):
            return p
    return None


_DEBUG_LOG = os.path.join(_DATA, "debug.log")
def _log(msg):
    """Append a diagnostic line to ~/.pgp_messenger/debug.log (best-effort)."""
    try:
        with open(_DEBUG_LOG, "a", encoding="utf-8") as f:
            f.write(f"{datetime.datetime.now().strftime('%H:%M:%S')}  {msg}\n")
    except Exception:
        pass


# If the process dies on a native fault (access violation etc.), dump the exact
# Python stack to ~/.pgp_messenger/fault.log so the crash is diagnosable.
try:
    import faulthandler
    _FAULT_FH = open(os.path.join(_DATA, "fault.log"), "w")
    faulthandler.enable(_FAULT_FH)
except Exception:
    pass


def _load(path, default):
    try:
        if os.path.exists(path):
            with open(path) as f:
                return json.load(f)
    except Exception:
        pass
    return default


def _save(path, data):
    with open(path, "w") as f:
        json.dump(data, f, indent=2)


# ─── PGP ──────────────────────────────────────────────────────────
class PGP:
    @staticmethod
    def _maybe_unlock(priv, passphrase, fn):
        """Run fn(priv) with the key unlocked if it is password-protected."""
        if priv.is_protected:
            with priv.unlock(passphrase or ""):
                return fn(priv)
        return fn(priv)

    @staticmethod
    def gen_key(name, email="", passphrase=""):
        k = pgpy.PGPKey.new(PubKeyAlgorithm.RSAEncryptOrSign, 4096)
        uid = pgpy.PGPUID.new(name, email=email)
        k.add_uid(uid,
            usage={KeyFlags.Sign, KeyFlags.EncryptCommunications, KeyFlags.EncryptStorage},
            hashes=[HashAlgorithm.SHA256, HashAlgorithm.SHA512],
            ciphers=[SymmetricKeyAlgorithm.AES256],
            compression=[CompressionAlgorithm.ZLIB, CompressionAlgorithm.Uncompressed])
        if passphrase:
            k.protect(passphrase, SymmetricKeyAlgorithm.AES256, HashAlgorithm.SHA256)
        return str(k), str(k.pubkey)

    @staticmethod
    def is_protected(armored):
        try:
            k, _ = pgpy.PGPKey.from_blob(armored)
            return bool(k.is_protected)
        except Exception:
            return False

    @staticmethod
    def set_password(priv_armored, new_pass, current_pass=None):
        k, _ = pgpy.PGPKey.from_blob(priv_armored)

        def _protect(key):
            key.protect(new_pass, SymmetricKeyAlgorithm.AES256, HashAlgorithm.SHA256)
            return None
        PGP._maybe_unlock(k, current_pass, _protect)
        return str(k)

    @staticmethod
    def set_expiry(priv_armored, expiry_dt, current_pass=None):
        """Re-sign the primary UID with a new expiration (datetime or None)."""
        k, _ = pgpy.PGPKey.from_blob(priv_armored)
        uid = next(iter(k.userids), None)
        if uid is None:
            raise ValueError("key has no user id")
        name, email = uid.name, (uid.email or "")

        def _resign(key):
            key.del_uid(name)
            nu = pgpy.PGPUID.new(name, email=email)
            kw = dict(
                usage={KeyFlags.Sign, KeyFlags.EncryptCommunications, KeyFlags.EncryptStorage},
                hashes=[HashAlgorithm.SHA256, HashAlgorithm.SHA512],
                ciphers=[SymmetricKeyAlgorithm.AES256],
                compression=[CompressionAlgorithm.ZLIB, CompressionAlgorithm.Uncompressed])
            if expiry_dt is not None:
                exp = expiry_dt
                # match the key's tz-awareness (pgpy stores UTC-aware datetimes)
                if exp.tzinfo is None and key.created.tzinfo is not None:
                    exp = exp.replace(tzinfo=key.created.tzinfo)
                delta = exp - key.created
                if delta.total_seconds() <= 0:
                    raise ValueError("expiry must be after the key creation date")
                kw["key_expiration"] = delta
            key.add_uid(nu, **kw)
            return None
        PGP._maybe_unlock(k, current_pass, _resign)
        return str(k), str(k.pubkey)

    @staticmethod
    def encrypt(text, pub_armored, sign_priv_armored=None, passphrase=None):
        pub, _ = pgpy.PGPKey.from_blob(pub_armored)
        msg = pgpy.PGPMessage.new(text)
        if sign_priv_armored:
            sp, _ = pgpy.PGPKey.from_blob(sign_priv_armored)
            sig = PGP._maybe_unlock(sp, passphrase, lambda k: k.sign(msg))
            msg |= sig
        return str(pub.encrypt(msg))

    @staticmethod
    def sign(text, priv_armored, passphrase=None):
        priv, _ = pgpy.PGPKey.from_blob(priv_armored)
        msg = pgpy.PGPMessage.new(text, cleartext=True)
        sig = PGP._maybe_unlock(priv, passphrase, lambda k: k.sign(msg))
        msg |= sig
        return str(msg)

    @staticmethod
    def decrypt(cipher_armored, priv_armored, passphrase=None):
        priv, _ = pgpy.PGPKey.from_blob(priv_armored)
        enc = pgpy.PGPMessage.from_blob(cipher_armored)
        m = PGP._maybe_unlock(priv, passphrase, lambda k: k.decrypt(enc).message)
        return m.decode("utf-8") if isinstance(m, (bytes, bytearray)) else str(m)

    @staticmethod
    def decrypt_verify(cipher_armored, priv_armored, known_pubs=(), passphrase=None):
        """Decrypt, then verify the signature against known public keys.
        Returns (plaintext, signer_name_or_None)."""
        priv, _ = pgpy.PGPKey.from_blob(priv_armored)
        enc = pgpy.PGPMessage.from_blob(cipher_armored)
        dec = PGP._maybe_unlock(priv, passphrase, lambda k: k.decrypt(enc))
        m = dec.message
        text = m.decode("utf-8") if isinstance(m, (bytes, bytearray)) else str(m)
        signer = None
        for name, pub_arm in known_pubs:
            try:
                pub, _ = pgpy.PGPKey.from_blob(pub_arm)
                if pub.verify(dec):
                    signer = name
                    break
            except Exception:
                pass
        return text, signer

    @staticmethod
    def verify_clearsigned(text, known_pubs=()):
        """Verify a clearsigned message; returns (cleartext, signer_or_None)."""
        msg = pgpy.PGPMessage.from_blob(text)
        body = msg.message
        if isinstance(body, (bytes, bytearray)):
            body = body.decode("utf-8", "replace")
        signer = None
        for name, pub_arm in known_pubs:
            try:
                pub, _ = pgpy.PGPKey.from_blob(pub_arm)
                if pub.verify(msg):
                    signer = name
                    break
            except Exception:
                pass
        return body, signer

    @staticmethod
    def key_info(armored):
        info = {"name": "", "email": "", "keyid": "", "fingerprint": "",
                "created": None, "expires": None, "status": "", "protected": False}
        try:
            k, _ = pgpy.PGPKey.from_blob(armored)
            uid = next(iter(k.userids), None)
            info["name"] = uid.name if uid else ""
            info["email"] = uid.email if uid else ""
            info["keyid"] = k.fingerprint.keyid
            info["fingerprint"] = str(k.fingerprint)
            info["protected"] = bool(k.is_protected)
            info["created"] = k.created
            exp = None
            try:
                si = uid.selfsig if uid else None
                if si is not None and si.key_expiration:
                    exp = k.created + si.key_expiration
            except Exception:
                exp = None
            info["expires"] = exp
            now = datetime.datetime.now(exp.tzinfo) if exp else datetime.datetime.now()
            if exp is None:
                info["status"] = "valid (no expiry)"
            elif exp < now:
                info["status"] = "expired"
            else:
                info["status"] = "valid"
        except Exception:
            pass
        return info


def _token_serial(fields):
    """True only if field 15 holds a real card serial. gpg uses single-char
    markers there: '#' = offline stub, '+' = secret available locally. A card
    serial is a long token id, so require length > 1 and not a marker."""
    if len(fields) > 14:
        v = fields[14].strip()
        return len(v) > 1 and v not in ("#", "+")
    return False


# ─── OpenPGP smartcard via GnuPG (Nitrokey / YubiKey) ─────────────
class Card:
    _NOWIN = 0x08000000 if os.name == "nt" else 0   # CREATE_NO_WINDOW

    @staticmethod
    def gpg():
        return shutil.which("gpg") or shutil.which("gpg2")

    @staticmethod
    def available():
        return Card.gpg() is not None

    @staticmethod
    def _run(args, stdin_text=None, timeout=120):
        g = Card.gpg()
        if not g:
            raise RuntimeError("GnuPG (gpg) is not installed.")
        # A --windowed PyInstaller build has no console, so the parent's
        # stdin/stdout/stderr handles are invalid; a gpg call that doesn't redirect
        # them fails with "WinError 6: the handle is invalid" (which is why the exe
        # silently failed to list/import keys). capture_output covers stdout/stderr;
        # redirect stdin too — DEVNULL when we have nothing to feed it.
        kw = dict(capture_output=True, text=True, encoding="utf-8", errors="replace",
                  timeout=timeout, creationflags=Card._NOWIN)
        if stdin_text is not None:
            kw["input"] = stdin_text
        else:
            kw["stdin"] = subprocess.DEVNULL
        return subprocess.run([g] + args, **kw)

    @staticmethod
    def status():
        try:
            r = Card._run(["--card-status"])
        except Exception as e:
            return {"present": False, "error": str(e)}
        out = (r.stdout or "") + "\n" + (r.stderr or "")
        low = out.lower()
        if "no such device" in low or "card not available" in low or \
           "selecting card failed" in low:
            return {"present": False}
        info = {"present": True, "raw": r.stdout}
        labels = {
            "Serial number": "serial",
            "Name of cardholder": "name",
            "Version": "version",
            "Signature key": "sig_fpr",
            "Encryption key": "enc_fpr",
            "Authentication key": "auth_fpr",
            "PIN retry counter": "pin_retries",
            "Reader": "reader",
        }
        for line in r.stdout.splitlines():
            if ":" not in line:
                continue
            head, _, val = line.partition(":")
            head = head.rstrip(". ").strip()
            val = val.strip()
            for lbl, key in labels.items():
                if head == lbl:
                    if key.endswith("_fpr"):
                        val = val.replace(" ", "")
                    info[key] = val
                    break
        return info

    @staticmethod
    def export_pubkey(fpr):
        if not fpr:
            return ""
        try:
            r = Card._run(["--armor", "--export", fpr])
            return (r.stdout or "").strip() if r.returncode == 0 else ""
        except Exception:
            return ""

    @staticmethod
    def sign(text, fpr=None):
        args = ["--armor", "--clearsign"]
        if fpr:
            args += ["--local-user", fpr]
        r = Card._run(args, stdin_text=text)
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError((r.stderr or "signing failed").strip())
        return r.stdout

    @staticmethod
    def decrypt(armored):
        """Decrypt (and verify) using the keyring/card; returns (plaintext, signer)."""
        r = Card._run(["--decrypt", "--status-fd", "2"], stdin_text=armored)
        text = r.stdout or ""
        if r.returncode != 0 and not text:
            raise RuntimeError((r.stderr or "decryption failed").strip())
        signer = None
        for line in (r.stderr or "").splitlines():
            if "Good signature from" in line and '"' in line:
                signer = line.split('"')[1]            # the quoted UID
                break
        return text, signer

    # ── keyring listing ───────────────────────────────────────────
    @staticmethod
    def _parse_uid(uid):
        email = ""
        if "<" in uid and ">" in uid:
            email = uid[uid.index("<") + 1:uid.index(">")]
        name = uid.split("<")[0].split("(")[0].strip()
        return name, email

    @staticmethod
    def _epoch(s):
        try:
            return datetime.datetime.fromtimestamp(int(s)) if s else None
        except Exception:
            return None

    @staticmethod
    def _list(secret):
        flag = "--list-secret-keys" if secret else "--list-keys"
        try:
            r = Card._run(["--with-colons", "--fixed-list-mode", flag])
        except Exception:
            return []
        keys, cur = [], None
        for line in (r.stdout or "").splitlines():
            f = line.split(":")
            rec = f[0]
            if rec in ("pub", "sec"):
                if cur:
                    keys.append(cur)
                cur = {"fpr": "", "keyid": f[4] if len(f) > 4 else "",
                       "created": Card._epoch(f[5] if len(f) > 5 else ""),
                       "expires": Card._epoch(f[6] if len(f) > 6 else ""),
                       "name": "", "email": "", "uids": [],
                       "secret": rec == "sec", "on_card": False}
                if rec == "sec" and _token_serial(f):
                    cur["on_card"] = True
            elif rec in ("ssb", "sub") and cur and _token_serial(f):
                # a (sub)key whose private part lives on inserted hardware
                cur["on_card"] = True
            elif rec == "fpr" and cur and not cur["fpr"]:
                cur["fpr"] = f[9] if len(f) > 9 else ""
            elif rec == "uid" and cur and len(f) > 9:
                cur["uids"].append(f[9])
                if not cur["name"]:
                    cur["name"], cur["email"] = Card._parse_uid(f[9])
        if cur:
            keys.append(cur)
        return keys

    @staticmethod
    def list_secret():
        return Card._list(True)

    @staticmethod
    def list_public():
        return Card._list(False)

    @staticmethod
    def has_secret():
        return bool(Card.list_secret())

    # ── import / export / delete ──────────────────────────────────
    @staticmethod
    def import_key(armored):
        r = Card._run(["--import", "--status-fd", "1"], stdin_text=armored)
        out = (r.stdout or "") + (r.stderr or "")
        if r.returncode != 0 and "IMPORT_OK" not in out and "imported" not in out.lower() \
           and "unchanged" not in out.lower():
            raise RuntimeError((r.stderr or "import failed").strip())
        return out

    @staticmethod
    def export_pub(fpr):
        r = Card._run(["--armor", "--export", fpr])
        return (r.stdout or "").strip()

    @staticmethod
    def export_secret(fpr):
        r = Card._run(["--armor", "--export-secret-keys", fpr])
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError((r.stderr or "export failed").strip())
        return r.stdout

    @staticmethod
    def delete_pub(fpr):
        r = Card._run(["--batch", "--yes", "--delete-keys", fpr])
        if r.returncode != 0:
            raise RuntimeError((r.stderr or "delete failed").strip())

    @staticmethod
    def delete_secret(fpr):
        r = Card._run(["--batch", "--yes", "--delete-secret-and-public-key", fpr])
        if r.returncode != 0:
            raise RuntimeError((r.stderr or "delete failed").strip())

    # ── generate / crypto ─────────────────────────────────────────
    @staticmethod
    def generate(name, email="", passphrase=""):
        params = ["Key-Type: RSA", "Key-Length: 4096",
                  "Subkey-Type: RSA", "Subkey-Length: 4096",
                  f"Name-Real: {name}"]
        if email:
            params.append(f"Name-Email: {email}")
        params.append("Expire-Date: 0")
        params.append(f"Passphrase: {passphrase}" if passphrase else "%no-protection")
        params.append("%commit")
        r = Card._run(["--batch", "--status-fd", "2", "--gen-key"],
                      stdin_text="\n".join(params) + "\n", timeout=600)
        if r.returncode != 0:
            raise RuntimeError((r.stderr or "key generation failed").strip())
        return (r.stdout or "") + (r.stderr or "")

    @staticmethod
    def encrypt(text, recipients, sign_fpr=None):
        args = ["--armor", "--encrypt", "--always-trust"]
        for rcp in recipients:
            args += ["--recipient", rcp]
        if sign_fpr:
            args += ["--sign", "--local-user", sign_fpr]
        r = Card._run(args, stdin_text=text)
        if r.returncode != 0 or not r.stdout:
            raise RuntimeError((r.stderr or "encryption failed").strip())
        return r.stdout

    # ── card admin (hardware) ─────────────────────────────────────
    @staticmethod
    def _card_run(stdin_text, edit=True, timeout=180):
        sub = "--card-edit" if edit else "--change-pin"
        return Card._run(["--command-fd", "0", "--status-fd", "2", sub],
                         stdin_text=stdin_text, timeout=timeout)

    @staticmethod
    def card_change_pin(which="1"):
        # 1 = user PIN, 2 = unblock (reset code/admin), 3 = admin PIN
        return Card._card_run(f"{which}\nQ\n", edit=False)

    @staticmethod
    def card_set_name(surname, given):
        return Card._card_run(f"admin\nname\n{surname}\n{given}\nquit\n")

    @staticmethod
    def card_set_url(url):
        return Card._card_run(f"admin\nurl\n{url}\nquit\n")

    @staticmethod
    def card_fetch():
        return Card._card_run("fetch\nquit\n")

    @staticmethod
    def keytocard(key_fpr, slot="2"):
        # move the (sub)key into a card slot: 1=sign 2=encrypt 3=auth
        cmds = f"keytocard\n{slot}\nsave\n"
        r = Card._run(["--command-fd", "0", "--status-fd", "2",
                       "--edit-key", key_fpr], stdin_text=cmds, timeout=180)
        return r


# Alias: the class is really a full GnuPG engine now.
GPG = Card


# ─── Scrollable canvas ────────────────────────────────────────────
class ScrollFrame(tk.Frame):
    _instances = []
    _wheel_bound = False

    def __init__(self, parent, bg=BK, bottom=False, thumb_side="right", **kw):
        super().__init__(parent, bg=bg, **kw)
        self._bg = bg
        self._bottom = bottom
        self._thumb_side = thumb_side
        self._canvas_h = 0
        self._first, self._last = 0.0, 1.0
        self._c  = tk.Canvas(self, bg=bg, highlightthickness=0, bd=0)
        self._c.configure(yscrollcommand=self._yset)
        self._c.pack(side="left", fill="both", expand=True)
        self.inner = tk.Frame(self._c, bg=bg)
        if bottom:
            # spacer pushes message rows to the bottom when they don't fill the view
            tk.Frame(self.inner, bg=bg).pack(side="top", fill="both", expand=True)
        self._win  = self._c.create_window((0, 0), window=self.inner, anchor="nw")
        # slim rounded-pill scrollbar thumb (overlaid, drag-enabled)
        self._thumb = tk.Canvas(self, bg=self._bg, highlightthickness=0, bd=0, width=5)
        self._thumb.bind("<Button-1>",  self._thumb_press)
        self._thumb.bind("<B1-Motion>", self._thumb_drag)
        self.inner.bind("<Configure>", self._on_inner)
        self._c.bind("<Configure>",    self._on_canvas)
        # one global wheel handler routes to whichever ScrollFrame is under the
        # pointer — so multiple scroll areas (sidebar + panels) never conflict
        ScrollFrame._instances.append(self)
        if not ScrollFrame._wheel_bound:
            self.bind_all("<MouseWheel>", ScrollFrame._route_wheel)
            ScrollFrame._wheel_bound = True

    @staticmethod
    def _route_wheel(e):
        try:
            node = e.widget.winfo_containing(e.x_root, e.y_root)
        except Exception:
            node = None
        ScrollFrame._instances = [s for s in ScrollFrame._instances if s.winfo_exists()]
        while node is not None:
            for sf in ScrollFrame._instances:
                if node is sf._c or node is sf.inner or node is sf:
                    return sf._scroll(e)
            node = getattr(node, "master", None)

    def _on_inner(self, _):
        self._c.configure(scrollregion=self._c.bbox("all"))
        self._reflow()

    def _on_canvas(self, e):
        self._c.itemconfig(self._win, width=e.width)
        self._canvas_h = e.height
        self._reflow()
        self._draw_thumb()

    def _reflow(self):
        if not self._bottom:
            return
        ch = self.inner.winfo_reqheight()
        self._c.itemconfig(self._win, height=max(ch, self._canvas_h))

    # ── custom scrollbar ──
    def _yset(self, first, last):
        self._first, self._last = float(first), float(last)
        self._draw_thumb()

    def _draw_thumb(self):
        if self._first <= 0.0 and self._last >= 1.0:
            self._thumb.place_forget()
            return
        track = self.winfo_height()
        if track <= 1:
            return
        pad = 9
        view = self._last - self._first            # visible fraction of the content
        # thumb length: proportional to the view, kept within a slim min/max band
        h = int(max(22, min(view * track, 100)))
        # precise position: the thumb travels only the *usable* track, so it sits
        # exactly at the top when scrolled to the top and exactly at the bottom when
        # scrolled to the bottom (no early-clamp drift).
        usable = max(0, track - 2 * pad - h)
        scroll_range = 1.0 - view                  # how far `first` can travel (0 → range)
        progress = (self._first / scroll_range) if scroll_range > 1e-9 else 0.0
        progress = min(1.0, max(0.0, progress))
        top = int(pad + progress * usable)
        W = 4                                      # slim
        if self._thumb_side == "left":
            self._thumb.place(relx=0.0, x=3, y=top, width=W, height=h, anchor="nw")
        else:
            self._thumb.place(relx=1.0, x=-3, y=top, width=W, height=h, anchor="ne")
        self._thumb.delete("all")
        if _HAS_PIL:
            # antialiased rounded pill (radius = half-width → fully curved ends)
            self._thumb_img = _rrect_img(W, h, W / 2.0, SCRL, self._bg)
            self._thumb.create_image(0, 0, image=self._thumb_img, anchor="nw")
        else:
            _round_rect(self._thumb, 0, 0, W, h, W / 2.0, fill=SCRL, outline="")
        try: self.tk.call("raise", self._thumb._w)   # raise the widget (Canvas.lift is tag_raise)
        except Exception: pass
        # auto-hide after 4s of no scroll activity
        if getattr(self, "_hide_job", None):
            try: self.after_cancel(self._hide_job)
            except Exception: pass
        self._hide_job = self.after(4000, self._thumb.place_forget)

    def _thumb_press(self, e):
        self._drag_y = e.y_root
        self._drag_first = self._first

    def _thumb_drag(self, e):
        # map the drag over the same usable travel as _draw_thumb so the thumb
        # follows the cursor 1:1 (pad = 9, matching _draw_thumb)
        track = self.winfo_height()
        h = self._thumb.winfo_height()
        usable = track - 2 * 9 - h
        scroll_range = 1.0 - (self._last - self._first)
        if usable > 0 and scroll_range > 1e-9:
            new_first = self._drag_first + (e.y_root - self._drag_y) / usable * scroll_range
            self._c.yview_moveto(min(max(0.0, new_first), 1.0))

    def _scroll(self, e):
        # eased momentum scrolling: accumulate a target and glide toward it
        try:
            box = self._c.bbox("all")
            if not box:
                return
            total = box[3] - box[1]
            view = self._c.winfo_height()
            if total <= view:
                return
            maxtop = total - view
            cur = self._first * total
            base = getattr(self, "_starget", cur)
            self._starget = min(max(0.0, base - (e.delta / 120) * 70.0), maxtop)
            if not getattr(self, "_sanim", False):
                self._sanim = True
                self._sstep()
        except tk.TclError:
            pass

    def _sstep(self):
        try:
            box = self._c.bbox("all")
            if not box:
                self._sanim = False
                return
            total = box[3] - box[1]
            cur = self._first * total
            diff = self._starget - cur
            if abs(diff) < 1.0:
                self._sanim = False
                return
            prev = self._first
            self._c.yview_moveto(min(max(0.0, (cur + diff * 0.28) / total), 1.0))
            # stop if we hit a scroll boundary (view can't move toward the target)
            if abs(self._first - prev) < 1e-4:
                self._starget = self._first * total
                self._sanim = False
                return
            self.after(12, self._sstep)
        except tk.TclError:
            self._sanim = False

    def scroll_bottom(self):
        self.update_idletasks()
        self._c.yview_moveto(1.0)

    def scroll_top(self):
        self.update_idletasks()
        self._c.yview_moveto(0.0)

    def bind_scroll_to(self, widget):
        widget.bind("<MouseWheel>", self._scroll)


# ─── Rounded button (same height as input) ────────────────────────
BTN_H = 34
RAD_W = 16    # corner radius for inputs / search pill (near-pill)
RAD_BTN = 8   # corner radius for buttons (a little less curved than inputs)
RAD_WIN = 24  # corner diameter for the app window (≈12px radius, Win11-like)
APP_W = 860
APP_H = 540

APP_VERSION = "0.5.6"
GITHUB_REPO = "PGPM-OPENSOURCE/OPENSOURCE-PGP-MESSANGER"

def _make_btn(parent, text, command, w=150, pbg=None, bold=False):
    pbg = pbg or parent.cget("bg")
    cv = tk.Canvas(parent, width=w, height=BTN_H, bg=pbg,
                   highlightthickness=0, bd=0, takefocus=0)
    img_n = _rrect_img(w, BTN_H, RAD_BTN, INP, pbg, outline=DIV) if _HAS_PIL else None
    img_h = _rrect_img(w, BTN_H, RAD_BTN, HOV, pbg, outline=DIV) if _HAS_PIL else None
    if img_n is not None:
        cv._imgs = (img_n, img_h)
        bgid = cv.create_image(0, 0, image=img_n, anchor="nw")
    else:
        bgid = _round_rect(cv, 1, 1, w - 1, BTN_H - 1, RAD_BTN, fill=INP, outline=DIV)
    tid = cv.create_text(w // 2, BTN_H // 2 + 1, text=text.upper(),
                         font=(UI, 9, "bold") if bold else (UI, 9), fill=WT)
    st = {"on": True}

    def _setbg(hover):
        if img_n is not None:
            cv.itemconfigure(bgid, image=(img_h if hover else img_n))
        else:
            cv.itemconfigure(bgid, fill=(HOV if hover else INP))

    def click(_=None):
        if st["on"]:
            command()

    def enter(_):
        if st["on"]:
            _setbg(True)
            cv.itemconfigure(tid, fill=WT)
            cv.configure(cursor="hand2")

    def leave(_):
        _setbg(False)
        cv.itemconfigure(tid, fill=WT if st["on"] else T4)
        cv.configure(cursor="")

    cv.bind("<Button-1>", click)
    cv.bind("<Enter>", enter)
    cv.bind("<Leave>", leave)

    cv.set_text    = lambda t: cv.itemconfigure(tid, text=t.upper())
    def _set_enabled(on):
        st["on"] = on
        cv.itemconfigure(tid, fill=WT if on else T4)
    cv.set_enabled = _set_enabled
    return cv


# ─── Modern rounded dropdown with a styled popup list ─────────────
def _dropdown(parent, labels, default=None, pbg=None, width=150):
    pbg = pbg or parent.cget("bg")
    var = tk.StringVar(value=default or (labels[0] if labels else ""))
    cv = tk.Canvas(parent, width=width, height=32, bg=pbg,
                   highlightthickness=0, bd=0, cursor="hand2")
    chev = _render_icon("chevron", 14, T2, pbg) if _HAS_PIL else None
    state = {"pop": None}

    def draw(_=None):
        w = cv.winfo_width()
        if w <= 1:
            return
        _round_bg(cv, INP, 12, outline=DIV)
        cv.delete("fg")
        cv.create_text(13, 16, text=var.get(), anchor="w", font=FM, fill=T1, tags="fg")
        if chev is not None:
            cv._chev = chev
            cv.create_image(w - 15, 16, image=chev, tags="fg")
    cv.bind("<Configure>", draw)
    var.trace_add("write", lambda *_: draw())

    def close(_=None):
        b = state.get("bind")
        if b:
            try: b[0].unbind("<Button-1>", b[1])
            except Exception: pass
            state["bind"] = None
        if state["pop"] is not None:
            try: state["pop"].destroy()
            except Exception: pass
            state["pop"] = None

    def toggle(_=None):
        if state["pop"] is not None:
            close(); return
        cv.update_idletasks()
        rowh, vis = 32, min(max(1, len(labels)), 7)
        w = max(width, cv.winfo_width())
        ph = vis * rowh + 2
        x = cv.winfo_rootx()
        below = cv.winfo_rooty() + cv.winfo_height() + 3
        # flip the popup above the field if it would spill off the bottom
        if below + ph > cv.winfo_screenheight() - 8:
            y = cv.winfo_rooty() - ph - 3
        else:
            y = below
        top = tk.Toplevel(cv); top.overrideredirect(True); top.configure(bg=DIV)
        top.geometry(f"{w}x{ph}+{x}+{y}")
        state["pop"] = top
        canvas = tk.Canvas(top, bg=INP, highlightthickness=0, bd=0)
        canvas.pack(fill="both", expand=True, padx=1, pady=1)
        inner = tk.Frame(canvas, bg=INP)
        iwin = canvas.create_window((0, 0), window=inner, anchor="nw")

        def wheel(e):
            try: canvas.yview_scroll(int(-e.delta / 120), "units")
            except tk.TclError: pass

        for lb in labels:
            r = tk.Label(inner, text=lb, font=FM, bg=INP, fg=T1, anchor="w",
                         padx=13, pady=7, cursor="hand2")
            r.pack(fill="x")
            r.bind("<Enter>", lambda e, q=r: q.configure(bg=HOV))
            r.bind("<Leave>", lambda e, q=r: q.configure(bg=INP))
            r.bind("<Button-1>", lambda e, l=lb: (var.set(l), close(), "break")[-1])
            r.bind("<MouseWheel>", wheel)
        inner.update_idletasks()
        canvas.configure(scrollregion=canvas.bbox("all"))
        canvas.itemconfig(iwin, width=w - 2)
        canvas.bind("<MouseWheel>", wheel)
        # close on click anywhere in the main window (no grab → cannot lock input)
        rw = cv.winfo_toplevel()
        bid = rw.bind("<Button-1>", close, add="+")
        state["bind"] = (rw, bid)
        top.bind("<Escape>", close)

    cv.bind("<Button-1>", toggle)
    cv._var = var
    return cv


# ─── Rounded input box matching button height ─────────────────────
def _make_input(parent, placeholder="", pbg=None):
    pbg = pbg or parent.cget("bg")
    # explicit small width so fill/expand grows it; default 378 would push siblings off-screen
    cv = tk.Canvas(parent, width=10, height=BTN_H, bg=pbg,
                   highlightthickness=0, bd=0, takefocus=0)
    txt = tk.Text(cv, font=FM, bg=INP, fg=T1,
                  insertbackground=T1, relief="flat", bd=0,
                  highlightthickness=0, wrap="none")
    win = cv.create_window(0, 0, window=txt, anchor="w")

    def redraw(_=None):
        w = cv.winfo_width()
        if w <= 1:
            return
        _round_bg(cv, INP, RAD_W, outline=DIV)
        cv.coords(win, 12, BTN_H // 2)
        cv.itemconfig(win, width=w - 24, height=BTN_H - 12)

    cv.bind("<Configure>", redraw)
    cv.bind("<Button-1>", lambda _: txt.focus_set())

    if placeholder:
        txt.insert("1.0", placeholder)
        txt.configure(fg=T3)

        def _fi(_):
            if txt.get("1.0", "end-1c") == placeholder:
                txt.delete("1.0", "end")
                txt.configure(fg=T1)

        def _fo(_):
            if not txt.get("1.0", "end-1c").strip():
                txt.insert("1.0", placeholder)
                txt.configure(fg=T3)

        txt.bind("<FocusIn>",  _fi)
        txt.bind("<FocusOut>", _fo)

    return cv, txt


# ─── Link label ───────────────────────────────────────────────────
def _link(parent, text, command, fg=T2, bg=BK):
    lbl = tk.Label(parent, text=text, font=FS, bg=bg, fg=fg, cursor="hand2")
    lbl.bind("<Button-1>", lambda _: command())
    lbl.bind("<Enter>",  lambda _, b=lbl: b.configure(fg=T1))
    lbl.bind("<Leave>",  lambda _, b=lbl: b.configure(fg=fg))
    return lbl


# ─── Selectable key/value row (mouse-copyable) ────────────────────
def _kv_row(parent, label, value, bg=BK):
    r = tk.Frame(parent, bg=bg)
    r.pack(fill="x", pady=2)
    tk.Label(r, text=label + ":", font=FS, bg=bg, fg=T3,
             width=13, anchor="w").pack(side="left")
    e = tk.Entry(r, font=FMO, fg=T1, bg=bg, readonlybackground=bg,
                 relief="flat", bd=0, highlightthickness=0,
                 disabledbackground=bg, state="normal")
    e.insert(0, value)
    e.configure(state="readonly")          # selectable + copyable, not editable
    e.pack(side="left", fill="x", expand=True)
    return e


# ─── Rounded entry field (Telegram-style) ─────────────────────────
def _make_entry(parent, var=None, bg=INP, pbg=None, show=""):
    pbg = pbg or parent.cget("bg")
    cv = tk.Canvas(parent, height=34, bg=pbg, highlightthickness=0, bd=0)
    e = tk.Entry(cv, font=FM, bg=bg, fg=T1, insertbackground=T1,
                 relief="flat", bd=0, highlightthickness=0, textvariable=var, show=show)
    win = cv.create_window(0, 0, window=e, anchor="w")

    def redraw(_=None):
        w = cv.winfo_width()
        if w <= 1:
            return
        _round_bg(cv, bg, 12, outline=DIV)
        cv.coords(win, 14, 17)
        cv.itemconfig(win, width=w - 28, height=22)
    cv.bind("<Configure>", redraw)
    cv.bind("<Button-1>", lambda _: e.focus_set())
    cv.pack(fill="x")
    return e


# ─── Rounded multi-line text field (Telegram-style) ───────────────
def _round_text(parent, height_px=None, font=FMO, fg=T2, pbg=None):
    pbg = pbg or parent.cget("bg")
    cv = tk.Canvas(parent, bg=pbg, highlightthickness=0, bd=0)
    if height_px:
        cv.configure(height=height_px)
    txt = tk.Text(cv, font=font, bg=INP, fg=fg, insertbackground=T1,
                  relief="flat", bd=0, highlightthickness=0, wrap="word")
    win = cv.create_window(0, 0, window=txt, anchor="nw")
    PAD, RAD = 12, 14
    cv._outline = DIV            # current border colour (turns white while a file hovers)

    def redraw(_=None):
        w, h = cv.winfo_width(), cv.winfo_height()
        if w <= 1 or h <= 1:
            return
        _round_bg(cv, INP, RAD, outline=cv._outline)
        cv.coords(win, PAD, PAD)
        cv.itemconfig(win, width=w - 2 * PAD, height=h - 2 * PAD)
    cv.bind("<Configure>", redraw)

    def set_border(color):
        if cv._outline != color:
            cv._outline = color
            redraw()
    cv._set_border = set_border
    cv._txt = txt
    return cv, txt


# ─── Rounded rectangle on a canvas ────────────────────────────────
def _round_rect(cv, x1, y1, x2, y2, r, **kw):
    r = max(0, min(r, (x2 - x1) / 2, (y2 - y1) / 2))
    pts = [
        x1 + r, y1,  x2 - r, y1,  x2, y1,
        x2, y1 + r,  x2, y2 - r,  x2, y2,
        x2 - r, y2,  x1 + r, y2,  x1, y2,
        x1, y2 - r,  x1, y1 + r,  x1, y1,
    ]
    return cv.create_polygon(pts, smooth=True, **kw)


# ─── Antialiased rounded-rect background (PIL; polygon fallback) ──
# Cached so the curved-corner textures are rendered once and reused instantly —
# no per-paint render and no sharp→curved flicker when a panel/tab is built.
_RRECT_CACHE = {}
_RRECT_ROOT = [None]

def _rrect_img(w, h, rad, fill, bg, outline=None, ow=1, ss=3):
    root = tk._default_root
    if root is not _RRECT_ROOT[0]:        # new Tk interpreter → drop stale images
        _RRECT_CACHE.clear()
        _RRECT_ROOT[0] = root
    key = (w, h, rad, fill, bg, outline, ow)
    img = _RRECT_CACHE.get(key)
    if img is not None:
        return img
    W, H = max(1, w) * ss, max(1, h) * ss
    im = Image.new("RGB", (W, H), bg)
    ImageDraw.Draw(im).rounded_rectangle(
        [0, 0, W - 1, H - 1], radius=rad * ss, fill=fill,
        outline=outline, width=(ow * ss if outline else 1))
    img = ImageTk.PhotoImage(im.resize((w, h), Image.LANCZOS))
    _RRECT_CACHE[key] = img
    return img


def _round_bg(cv, fill, rad, outline=None, tag="bgr"):
    """Paint a crisp, antialiased rounded-rect background immediately (cached),
    so corners are curved on the very first paint — no flicker."""
    w, h = cv.winfo_width(), cv.winfo_height()
    if w <= 1 or h <= 1:
        return
    cv.delete(tag)
    if _HAS_PIL:
        cv._bgimg = _rrect_img(w, h, rad, fill, cv["bg"], outline=outline)
        cv.create_image(0, 0, image=cv._bgimg, anchor="nw", tags=tag)
    else:
        _round_rect(cv, 1, 1, w - 1, h - 1, rad, fill=fill,
                    outline=outline or "", tags=tag)
    cv.tag_lower(tag)


# ─── Hand-drawn minimalist icons (monochrome, theme-coloured) ─────
def _icon_canvas(parent, size, draw, command, fg=T2, hover=T1, pbg=None):
    pbg = pbg or parent.cget("bg")
    cv = tk.Canvas(parent, width=size, height=size, bg=pbg,
                   highlightthickness=0, bd=0, takefocus=0, cursor="hand2")
    draw(cv, size, fg)
    cv.bind("<Button-1>", lambda _: command())
    cv.bind("<Enter>", lambda _: [cv.itemconfigure(i, fill=hover, outline=hover)
                                  for i in cv.find_withtag("ico")])
    cv.bind("<Leave>", lambda _: [cv.itemconfigure(i, fill=fg, outline=fg)
                                  for i in cv.find_withtag("ico")])
    cv._set = lambda c: [cv.itemconfigure(i, fill=c, outline=c)
                         for i in cv.find_withtag("ico")]
    return cv


def draw_search(cv, s, fg):
    # magnifier: ring + handle
    r = s * 0.30
    cx, cy = s * 0.42, s * 0.42
    cv.create_oval(cx - r, cy - r, cx + r, cy + r, outline=fg, width=2,
                   fill="", tags="ico")
    cv.create_line(cx + r * 0.75, cy + r * 0.75, s * 0.82, s * 0.82,
                   fill=fg, width=2, capstyle="round", tags="ico")


def draw_plus_circle(cv, s, fg):
    # circle outline with a centred plus
    m = s * 0.12
    cv.create_oval(m, m, s - m, s - m, outline=fg, width=2, fill="", tags="ico")
    c = s / 2
    e = s * 0.22
    cv.create_line(c - e, c, c + e, c, fill=fg, width=2, capstyle="round", tags="ico")
    cv.create_line(c, c - e, c, c + e, fill=fg, width=2, capstyle="round", tags="ico")


def draw_key(cv, s, fg):
    # key: ring (bow) on the left + shaft + two teeth
    r = s * 0.20
    cx, cy = s * 0.30, s * 0.50
    cv.create_oval(cx - r, cy - r, cx + r, cy + r, outline=fg, width=2,
                   fill="", tags="ico")
    sx = cx + r
    cv.create_line(sx, cy, s * 0.86, cy, fill=fg, width=2, capstyle="round", tags="ico")
    cv.create_line(s * 0.86, cy, s * 0.86, cy + s * 0.16, fill=fg, width=2,
                   capstyle="round", tags="ico")
    cv.create_line(s * 0.70, cy, s * 0.70, cy + s * 0.12, fill=fg, width=2,
                   capstyle="round", tags="ico")


def draw_trash(cv, s, fg):
    # trash can: lid line + handle + body + ribs
    cv.create_line(s * 0.22, s * 0.30, s * 0.78, s * 0.30, fill=fg, width=2,
                   capstyle="round", tags="ico")
    cv.create_line(s * 0.42, s * 0.22, s * 0.58, s * 0.22, fill=fg, width=2,
                   capstyle="round", tags="ico")
    # body sides + bottom
    cv.create_line(s * 0.30, s * 0.30, s * 0.34, s * 0.78, fill=fg, width=2,
                   capstyle="round", tags="ico")
    cv.create_line(s * 0.70, s * 0.30, s * 0.66, s * 0.78, fill=fg, width=2,
                   capstyle="round", tags="ico")
    cv.create_line(s * 0.34, s * 0.78, s * 0.66, s * 0.78, fill=fg, width=2,
                   capstyle="round", tags="ico")
    # ribs
    cv.create_line(s * 0.50, s * 0.38, s * 0.50, s * 0.70, fill=fg, width=2,
                   capstyle="round", tags="ico")


def draw_menu(cv, s, fg):
    # hamburger: three horizontal lines
    for fy in (0.32, 0.50, 0.68):
        cv.create_line(s * 0.18, s * fy, s * 0.82, s * fy, fill=fg, width=2,
                       capstyle="round", tags="ico")


_CANVAS_ICONS = {"search": draw_search, "plus": draw_plus_circle,
                 "key": draw_key, "trash": draw_trash, "menu": draw_menu}


# ─── High-quality PIL icons (supersampled + antialiased) ──────────
def _pl(d, p0, p1, w, c):
    """Round-capped line."""
    d.line([p0, p1], fill=c, width=int(w))
    rr = w / 2.0
    for (x, y) in (p0, p1):
        d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=c)


def _pil_search(d, s, c):
    w = s * 0.10
    r = s * 0.26
    cx, cy = s * 0.40, s * 0.40
    d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=c, width=int(w))
    _pl(d, (cx + r * 0.72, cy + r * 0.72), (s * 0.86, s * 0.86), w, c)


def _pil_plus(d, s, c):
    w = s * 0.09
    m = s * 0.10
    d.ellipse([m, m, s - m, s - m], outline=c, width=int(w))
    cc, e = s / 2, s * 0.22
    _pl(d, (cc - e, cc), (cc + e, cc), w, c)
    _pl(d, (cc, cc - e), (cc, cc + e), w, c)


def _pil_key(d, s, c):
    # upright key: ring bow with a hole + shaft + two teeth
    w = s * 0.085
    r = s * 0.19
    cx, cy = s * 0.50, s * 0.30
    d.ellipse([cx - r, cy - r, cx + r, cy + r], outline=c, width=int(w))  # bow (with hole)
    _pl(d, (cx, cy + r * 0.7), (cx, s * 0.86), w, c)            # shaft
    _pl(d, (cx, s * 0.74), (cx + s * 0.15, s * 0.74), w, c)     # tooth
    _pl(d, (cx, s * 0.84), (cx + s * 0.11, s * 0.84), w, c)     # tooth


def _pil_trash(d, s, c):
    # modern bin: rounded handle, solid lid bar, rounded can body with ribs
    w = s * 0.07
    d.rounded_rectangle([s * 0.40, s * 0.15, s * 0.60, s * 0.235],
                        radius=s * 0.035, outline=c, width=int(w))      # handle
    d.rounded_rectangle([s * 0.16, s * 0.255, s * 0.84, s * 0.325],
                        radius=s * 0.035, fill=c)                       # lid bar
    d.rounded_rectangle([s * 0.265, s * 0.36, s * 0.735, s * 0.84],
                        radius=s * 0.11, outline=c, width=int(w))       # can body
    for fx in (0.41, 0.50, 0.59):                                       # ribs
        _pl(d, (s * fx, s * 0.47), (s * fx, s * 0.73), w * 0.85, c)


def _pil_menu(d, s, c):
    w = s * 0.095
    for fy in (0.32, 0.50, 0.68):
        _pl(d, (s * 0.18, s * fy), (s * 0.82, s * fy), w, c)


def _pil_doc(d, s, c, w):
    # document page with a folded top-right corner
    x0, y0, x1, y1 = s * 0.22, s * 0.13, s * 0.66, s * 0.87
    fold = s * 0.15
    d.line([(x0, y0), (x0, y1), (x1, y1), (x1, y0 + fold),
            (x1 - fold, y0), (x0, y0)], fill=c, width=int(w), joint="curve")
    d.line([(x1 - fold, y0), (x1 - fold, y0 + fold), (x1, y0 + fold)],
           fill=c, width=int(w))


def _pil_sign(d, s, c):
    # document + small padlock badge (Sign/Encrypt)
    w = s * 0.08
    _pil_doc(d, s, c, w)
    d.arc([s * 0.55, s * 0.49, s * 0.80, s * 0.70], 180, 360, fill=c, width=int(w))
    d.rounded_rectangle([s * 0.51, s * 0.61, s * 0.83, s * 0.85],
                        radius=s * 0.05, fill=c)


def _pil_verify(d, s, c):
    # document + magnifier badge (Decrypt/Verify)
    w = s * 0.08
    _pil_doc(d, s, c, w)
    cx, cy, r = s * 0.62, s * 0.63, s * 0.14
    d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c)        # solid lens
    _pl(d, (cx + r * 0.72, cy + r * 0.72), (s * 0.88, s * 0.90), w, c)


def _pil_import(d, s, c):
    # down arrow into an open box / tray (Import)
    w = s * 0.085
    _pl(d, (s * 0.50, s * 0.14), (s * 0.50, s * 0.58), w, c)    # shaft
    _pl(d, (s * 0.34, s * 0.42), (s * 0.50, s * 0.58), w, c)    # arrow head
    _pl(d, (s * 0.66, s * 0.42), (s * 0.50, s * 0.58), w, c)
    d.line([(s * 0.20, s * 0.58), (s * 0.20, s * 0.82),
            (s * 0.80, s * 0.82), (s * 0.80, s * 0.58)],
           fill=c, width=int(w), joint="curve")


def _pil_more(d, s, c):
    # vertical 3-dot kebab menu
    r = s * 0.075
    for fy in (0.26, 0.50, 0.74):
        cx, cy = s * 0.50, s * fy
        d.ellipse([cx - r, cy - r, cx + r, cy + r], fill=c)


def _pil_back(d, s, c):
    # "<" chevron (back)
    w = s * 0.11
    _pl(d, (s * 0.60, s * 0.24), (s * 0.34, s * 0.50), w, c)
    _pl(d, (s * 0.34, s * 0.50), (s * 0.60, s * 0.76), w, c)


def _pil_chevron(d, s, c):
    # small downward chevron (dropdown indicator)
    w = s * 0.10
    _pl(d, (s * 0.30, s * 0.42), (s * 0.50, s * 0.62), w, c)
    _pl(d, (s * 0.50, s * 0.62), (s * 0.70, s * 0.42), w, c)


def _pil_keycard(d, s, c):
    # smartcard: rounded card + chip + two contact lines
    w = s * 0.07
    d.rounded_rectangle([s * 0.13, s * 0.27, s * 0.87, s * 0.73],
                        radius=s * 0.09, outline=c, width=int(w))
    d.rounded_rectangle([s * 0.22, s * 0.38, s * 0.40, s * 0.55],
                        radius=s * 0.035, fill=c)                  # chip
    _pl(d, (s * 0.50, s * 0.43), (s * 0.76, s * 0.43), w * 0.85, c)
    _pl(d, (s * 0.50, s * 0.55), (s * 0.68, s * 0.55), w * 0.85, c)


def _pil_copy(d, s, c):
    # two overlapping rounded rectangles (copy)
    w = s * 0.066
    d.rounded_rectangle([s * 0.30, s * 0.22, s * 0.66, s * 0.62],
                        radius=s * 0.06, outline=c, width=int(w))   # back sheet
    d.rounded_rectangle([s * 0.40, s * 0.38, s * 0.76, s * 0.78],
                        radius=s * 0.06, outline=c, width=int(w))   # front sheet


def _pil_shuffle(d, s, c):
    # shuffle: two crossing strands with arrowheads at the right ends
    w = s * 0.085
    L, R = s * 0.14, s * 0.80
    _pl(d, (L, s * 0.30), (R, s * 0.70), w, c)
    _pl(d, (L, s * 0.70), (R, s * 0.30), w, c)
    _pl(d, (R, s * 0.70), (R - s * 0.13, s * 0.70), w, c)   # lower arrowhead barbs
    _pl(d, (R, s * 0.70), (R, s * 0.57), w, c)
    _pl(d, (R, s * 0.30), (R - s * 0.13, s * 0.30), w, c)   # upper arrowhead barbs
    _pl(d, (R, s * 0.30), (R, s * 0.43), w, c)


_PIL_ICONS = {"search": _pil_search, "plus": _pil_plus,
              "key": _pil_key, "trash": _pil_trash, "menu": _pil_menu,
              "sign": _pil_sign, "verify": _pil_verify, "import": _pil_import,
              "more": _pil_more, "back": _pil_back, "chevron": _pil_chevron,
              "keycard": _pil_keycard, "copy": _pil_copy, "shuffle": _pil_shuffle,
              "encrypt": _pil_sign}   # fallback if Desktop\encrypt.png is missing


_CUSTOM_MASK = {}       # kind -> cropped alpha mask (L) or None
_CUSTOM_ICONS = {}      # (kind, size, color) -> PhotoImage

def _custom_mask(kind):
    if kind in _CUSTOM_MASK:
        return _CUSTOM_MASK[kind]
    mask = None
    entry = _CUSTOM_PNG.get(kind)
    # prefer the Desktop-synced copy, else a bundled fallback (so core icons like the
    # back button still work in the exe even without the user's Desktop file)
    candidates = []
    if entry:
        candidates = [entry[0], _resource(os.path.basename(entry[0]))]
    for path in candidates:
        if path and os.path.exists(path):
            try:
                im = Image.open(path).convert("RGBA")
                bbox = im.getchannel("A").getbbox()
                mask = (im.crop(bbox) if bbox else im).getchannel("A")
                break
            except Exception:
                mask = None
    _CUSTOM_MASK[kind] = mask
    return mask


def _custom_icon_img(kind, size, color):
    ck = (kind, size, color)
    if ck in _CUSTOM_ICONS:
        return _CUSTOM_ICONS[ck]
    mask = _custom_mask(kind)
    if mask is None:
        return None
    ss = 4
    S = size * ss
    mw, mh = mask.size
    scale = min(S / mw, S / mh)
    nw, nh = max(1, int(mw * scale)), max(1, int(mh * scale))
    m = mask.resize((nw, nh), Image.LANCZOS)
    canvas = Image.new("L", (S, S), 0)
    canvas.paste(m, ((S - nw) // 2, (S - nh) // 2))
    rgb = tuple(int(color[i:i + 2], 16) for i in (1, 3, 5))
    tinted = Image.new("RGBA", (S, S), rgb + (0,))
    tinted.putalpha(canvas)
    img = ImageTk.PhotoImage(tinted.resize((size, size), Image.LANCZOS))
    _CUSTOM_ICONS[ck] = img
    return img


def _render_icon(kind, size, color, bg=None, ss=4):
    img = _custom_icon_img(kind, size, color)   # custom png (key / keycard) if present
    if img is not None:
        return img
    # transparent RGBA so icons composite on any background (hover, pill, etc.)
    S = size * ss
    img = Image.new("RGBA", (S, S), (0, 0, 0, 0))
    _PIL_ICONS[kind](ImageDraw.Draw(img), S, color)
    img = img.resize((size, size), Image.LANCZOS)
    return ImageTk.PhotoImage(img)


# ─── Hover tooltip ────────────────────────────────────────────────
def _attach_tip(widget, text):
    st = {"win": None, "job": None}

    def show():
        if st["win"] is not None:
            return
        t = tk.Toplevel(widget)
        t.overrideredirect(True)
        t.configure(bg=DIV)
        tk.Label(t, text=text, font=(UI, 8), bg="#202022", fg=T1,
                 padx=8, pady=3).pack(padx=1, pady=1)
        t.update_idletasks()
        x = widget.winfo_rootx() + widget.winfo_width() // 2 - t.winfo_width() // 2
        y = widget.winfo_rooty() - t.winfo_height() - 5
        if y < 4:                                        # flip below if no room above
            y = widget.winfo_rooty() + widget.winfo_height() + 5
        t.geometry(f"+{max(2, x)}+{y}")
        st["win"] = t

    def hide():
        if st["job"]:
            try: widget.after_cancel(st["job"])
            except Exception: pass
            st["job"] = None
        if st["win"] is not None:
            try: st["win"].destroy()
            except Exception: pass
            st["win"] = None

    widget.bind("<Enter>", lambda _: st.update(job=widget.after(500, show)), add="+")
    widget.bind("<Leave>", lambda _: hide(), add="+")
    widget.bind("<Button-1>", lambda _: hide(), add="+")


def _icon_widget(parent, size, kind, command, fg=T2, hover=T1, pbg=None, tip=None):
    """High-quality icon button (PIL); falls back to canvas drawing."""
    pbg = pbg or parent.cget("bg")
    if _HAS_PIL:
        img_n = _render_icon(kind, size, fg, pbg)
        img_h = _render_icon(kind, size, hover, pbg)
        lbl = tk.Label(parent, image=img_n, bg=pbg, cursor="hand2", bd=0)
        lbl._imgs = (img_n, img_h)          # keep refs from GC
        lbl.bind("<Button-1>", lambda _: command())
        lbl.bind("<Enter>", lambda _: lbl.configure(image=img_h))
        lbl.bind("<Leave>", lambda _: lbl.configure(image=img_n))
        if tip:
            _attach_tip(lbl, tip)
        return lbl
    draw = _CANVAS_ICONS.get(kind, draw_search)
    w = _icon_canvas(parent, size, draw, command, fg, hover, pbg)
    if tip:
        _attach_tip(w, tip)
    return w


# ─── Offline "rewrite for anonymity": grammar/spelling + style normalizer ─────
_REWRITE_MAP = {
    # contractions → expanded (neutralises informal style)
    "i'm": "i am", "i'll": "i will", "i'd": "i would", "i've": "i have",
    "you're": "you are", "you'll": "you will", "you've": "you have", "you'd": "you would",
    "he's": "he is", "she's": "she is", "it's": "it is", "that's": "that is",
    "there's": "there is", "here's": "here is", "what's": "what is", "who's": "who is",
    "we're": "we are", "we'll": "we will", "we've": "we have", "we'd": "we would",
    "they're": "they are", "they'll": "they will", "they've": "they have", "they'd": "they would",
    "don't": "do not", "doesn't": "does not", "didn't": "did not", "can't": "cannot",
    "couldn't": "could not", "won't": "will not", "wouldn't": "would not",
    "shouldn't": "should not", "isn't": "is not", "aren't": "are not", "wasn't": "was not",
    "weren't": "were not", "haven't": "have not", "hasn't": "has not", "hadn't": "had not",
    "let's": "let us", "ain't": "is not",
    # common no-apostrophe contractions
    "im": "i am", "ive": "i have", "ill": "i will", "dont": "do not", "doesnt": "does not",
    "didnt": "did not", "cant": "cannot", "wont": "will not", "couldnt": "could not",
    "wouldnt": "would not", "shouldnt": "should not", "isnt": "is not", "arent": "are not",
    "wasnt": "was not", "youre": "you are", "theyre": "they are", "thats": "that is",
    "whats": "what is", "hes": "he is", "shes": "she is",
    # chat-speak → standard (a strong writing-style fingerprint)
    "u": "you", "ur": "your", "r": "are", "pls": "please", "plz": "please", "thx": "thanks",
    "ty": "thank you", "msg": "message", "msgs": "messages", "b4": "before", "l8r": "later",
    "gr8": "great", "cuz": "because", "tho": "though", "thru": "through", "nite": "night",
    "gud": "good", "dat": "that", "dis": "this", "wat": "what", "gonna": "going to",
    "wanna": "want to", "gotta": "have to", "kinda": "kind of", "dunno": "do not know",
    "lemme": "let me", "gimme": "give me",
    # common misspellings
    "teh": "the", "adn": "and", "recieve": "receive", "recieved": "received",
    "adress": "address", "definately": "definitely", "seperate": "separate",
    "occured": "occurred", "untill": "until", "wich": "which", "becuase": "because",
    "becos": "because", "thier": "their", "alot": "a lot", "beleive": "believe",
    "belive": "believe", "freind": "friend", "wierd": "weird", "tommorow": "tomorrow",
    "tomorow": "tomorrow", "wheter": "whether",
    # informal → formal
    "hi": "hello", "hey": "hello", "yeah": "yes", "yep": "yes", "yup": "yes",
    "nope": "no", "nah": "no", "thanks": "thank you", "thanx": "thank you",
    "asap": "as soon as possible", "fyi": "for your information", "btw": "by the way",
    "aka": "also known as", "approx": "approximately", "info": "information",
    "pic": "picture", "pics": "pictures",
    # g-dropped verbs (X-in → X-ing) — only forms that are NOT themselves real words
    # (so "cabin", "robin", "raisin" are deliberately left out)
    "goin": "going", "doin": "doing", "bein": "being", "makin": "making",
    "takin": "taking", "comin": "coming", "givin": "giving", "livin": "living",
    "lovin": "loving", "drivin": "driving", "ridin": "riding", "gettin": "getting",
    "settin": "setting", "sittin": "sitting", "puttin": "putting", "runnin": "running",
    "swimmin": "swimming", "winnin": "winning", "talkin": "talking", "walkin": "walking",
    "lookin": "looking", "workin": "working", "playin": "playing", "sayin": "saying",
    "payin": "paying", "tryin": "trying", "cryin": "crying", "buyin": "buying",
    "flyin": "flying", "dyin": "dying", "lyin": "lying", "feelin": "feeling",
    "dealin": "dealing", "stealin": "stealing", "hearin": "hearing", "wearin": "wearing",
    "sharin": "sharing", "starin": "staring", "stayin": "staying", "prayin": "praying",
    "knowin": "knowing", "showin": "showing", "growin": "growing", "throwin": "throwing",
    "blowin": "blowing", "followin": "following", "thinkin": "thinking",
    "drinkin": "drinking", "askin": "asking", "breakin": "breaking", "speakin": "speaking",
    "leavin": "leaving", "movin": "moving", "usin": "using", "losin": "losing",
    "callin": "calling", "tellin": "telling", "sendin": "sending", "readin": "reading",
    "writin": "writing", "waitin": "waiting", "meetin": "meeting", "needin": "needing",
    "somethin": "something", "nothin": "nothing", "anythin": "anything",
    "everythin": "everything", "mornin": "morning", "evenin": "evening",
    # texting slang / abbreviations → standard (strong style fingerprints)
    "wsp": "what is up", "sup": "what is up", "wassup": "what is up", "whatsup": "what is up",
    "hru": "how are you", "hbu": "how about you", "wbu": "what about you",
    "idk": "I do not know", "idc": "I do not care", "imo": "in my opinion",
    "imho": "in my opinion", "tbh": "to be honest", "ngl": "not going to lie",
    "fr": "for real", "rn": "right now", "omw": "on my way", "brb": "be right back",
    "gtg": "got to go", "ttyl": "talk to you later", "hmu": "contact me",
    "ikr": "I know right", "nvm": "never mind", "jk": "just kidding", "ofc": "of course",
    "obv": "obviously", "obvi": "obviously", "prob": "probably", "prolly": "probably",
    "congrats": "congratulations", "abt": "about", "rly": "really", "srsly": "seriously",
    "bday": "birthday", "fav": "favorite", "fave": "favorite", "ppl": "people",
    "tmrw": "tomorrow", "tmrrw": "tomorrow", "tonite": "tonight", "coz": "because",
    "becuz": "because", "ya": "you", "yall": "you all", "outta": "out of",
    "wanna": "want to", "gonna": "going to", "gotta": "have to", "hafta": "have to",
    # laughter / chat filler → removed
    "lol": "", "lmao": "", "rofl": "", "haha": "", "hahaha": "", "omg": "",
}
_PUNCT_MAP = {"‘": "'", "’": "'", "“": '"', "”": '"',
              "–": "-", "—": "-", "…": "...", " ": " ", "​": ""}

_KEEP_UPPER = {"BTC", "ETH", "XMR", "LTC", "USD", "EUR", "GBP", "PGP", "GPG",
               "RSA", "AES", "SHA", "VPN", "TOR", "IP", "API", "URL", "PIN",
               "OTP", "ID", "USA", "UK", "EU", "FBI", "CIA", "NSA", "OK"}


def _anonymize_text(text):
    """Fix grammar/spelling and strip writing-style fingerprints, fully offline.
    Conservative: leaves capitalised names, handles, URLs and digit tokens alone."""
    if not text or not text.strip():
        return text
    # 1) normalise unicode punctuation/spaces, then drop remaining non-ASCII (emoji)
    out = "".join(_PUNCT_MAP.get(ch, ch) for ch in text)
    out = out.encode("ascii", "ignore").decode("ascii")
    # 2) collapse 3+ repeated chars ("soooo"->"so") and runs of ! / ?
    out = re.sub(r"(.)\1{2,}", r"\1", out)
    out = re.sub(r"[!?]{2,}", lambda m: "?" if "?" in m.group(0) else "!", out)
    # 3) per word: rewrite known typos/chat-speak/contractions, de-shout ALL-CAPS.
    #    Only words on the curated map change — unknown words (names, bitcoin, …) are
    #    left exactly as-is, so meaning is never invented/altered.
    def fix(m):
        tok = m.group(0)
        low = tok.lower()
        if low in _REWRITE_MAP:
            return _REWRITE_MAP[low]
        # apostrophe g-drop is unambiguous: "drivin'" -> "driving", "goin'" -> "going"
        if low.endswith("in'") and low[:-3].isalpha() and len(low) >= 5:
            stem = tok[:-3]
            return stem + ("ING" if tok.isupper() else "ing")
        if tok in _KEEP_UPPER:
            return tok                          # keep tickers/acronyms (BTC, PGP, …)
        if tok.isalpha() and tok.isupper() and len(tok) > 1:
            return tok.lower()                 # de-shout ALL-CAPS (letters only, not key IDs)
        return tok
    # tokens may contain digits so leetspeak ("l8r", "b4", "gr8") matches as one word
    out = re.sub(r"[A-Za-z0-9][A-Za-z0-9']*", fix, out)
    # 4) capitalise sentence starts + standalone "i"
    res, cap = [], True
    for ch in out:
        if cap and ch.isalpha():
            res.append(ch.upper()); cap = False
        else:
            res.append(ch)
        if ch in ".!?":
            cap = True
    out = "".join(res)
    out = re.sub(r"\bi\b", "I", out)
    # 5) tidy whitespace + punctuation spacing
    out = re.sub(r"\s+", " ", out)
    out = re.sub(r"\s+([,.!?;:])", r"\1", out)
    out = re.sub(r"([,.!?;:])(?=\S)", r"\1 ", out)
    out = out.strip()
    if out and out[-1] not in ".!?":
        out += "."                            # finish with proper sentence punctuation
    return out


# ─── App ──────────────────────────────────────────────────────────
class App(tk.Tk):
    def __init__(self):
        super().__init__()
        self.title("PGPM")                  # taskbar label (avoid default "tk")
        try:
            import tkinter.font as _tkf
            _log(f"ui_font={UI!r} inter_available={'Inter' in _tkf.families()}")
        except Exception:
            pass
        if IS_WIN:
            self.overrideredirect(True)     # custom borderless chrome (Windows only)
        self.geometry(f"{APP_W}x{APP_H}")
        self.configure(bg=BK)
        self.option_add("*tearOff", False)
        self._dx = self._dy = 0
        self._priv_visible = False
        self._maximized = False
        self._restore_geo = None

        # GnuPG keyring is the source of truth (shared with Kleopatra/gpg)
        self.my_fpr = _load(_CONFIG, {}).get("my_fpr")
        self._secret = []
        self.contacts = []
        self.active  = None
        self._drag = None                 # in-flight contact drag-reorder state
        self._drop_targets = []           # message fields that accept dropped files
        self._search_filter = ""
        self._view = None                 # id of the panel currently shown (no-reload guard)
        self._card_present = None         # last known keycard presence (None = not yet probed)
        self._migrate_legacy()
        self._reload_keys()

        self._ui()
        self._set_icon()
        self.protocol("WM_DELETE_WINDOW", self._quit)   # native close → full exit
        self.center()
        if IS_WIN:
            self.after(80, self._round_corners)
            self.after(120, self._win_taskbar)
            self.after(220, self._enable_drop)          # accept dragged-in files
            if os.environ.get("PGPM_SELFTEST_DROP"):
                self.after(700, self._selftest_drop)    # headless drop stress-test → exits
            if os.environ.get("PGPM_DROPTEST_SERVE"):
                self.after(800, self._droptest_serve)   # real-drag test target → exits on load
            self.bind("<Map>", self._on_restore)        # keep rounded corners after restore
        self._start_card_watch()          # live plug/unplug detection
        self.after(1500, self._check_update)            # non-blocking update check
        # open silently — no auto card/setup prompt

    def center(self):
        self.update_idletasks()
        sw, sh = self.winfo_screenwidth(), self.winfo_screenheight()
        self.geometry(f"{APP_W}x{APP_H}+{(sw-APP_W)//2}+{(sh-APP_H)//2}")

    def _set_icon(self):
        """Window / taskbar / dock icon (all platforms) from icon.png."""
        path = _icon_path()
        if not path or not _HAS_PIL:
            return
        try:
            im = Image.open(path)
            im.thumbnail((256, 256), Image.LANCZOS)
            self._icon_img = ImageTk.PhotoImage(im)        # keep a ref from GC
            self.iconphoto(True, self._icon_img)
        except Exception:
            pass

    def _win_taskbar(self):
        """Give the borderless Windows window a real taskbar button + its own
        taskbar identity, so our icon appears there (overrideredirect hides it)."""
        try:
            import ctypes
            ctypes.windll.shell32.SetCurrentProcessExplicitAppUserModelID(
                "aaaaeer.pgpm")
        except Exception:
            pass
        try:
            import ctypes
            GWL_EXSTYLE, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW = -20, 0x40000, 0x80
            hwnd = self._hwnd()
            if not hwnd:
                return
            u32 = ctypes.windll.user32
            style = u32.GetWindowLongW(hwnd, GWL_EXSTYLE)
            u32.SetWindowLongW(hwnd, GWL_EXSTYLE,
                               (style & ~WS_EX_TOOLWINDOW) | WS_EX_APPWINDOW)
            self.withdraw()                    # re-show so the taskbar adopts the style
            self.after(10, self._reshow_taskbar)
        except Exception:
            pass

    def _reshow_taskbar(self):
        self.deiconify()
        self._round_corners(not self._maximized)

    def _quit(self):
        """Fully exit: stop the watch loop, tear down Tk, then hard-exit so no
        thread or child process keeps the app alive in the background."""
        self._closing = True
        try:
            job = getattr(self, "_card_watch_job", None)
            if job:
                self.after_cancel(job)
        except Exception:
            pass
        try:
            self.destroy()
        except Exception:
            pass
        os._exit(0)

    def _hwnd(self):
        try:
            import ctypes
            from ctypes import wintypes
            u32 = ctypes.windll.user32
            u32.GetAncestor.restype  = wintypes.HWND
            u32.GetAncestor.argtypes = [wintypes.HWND, wintypes.UINT]
            return u32.GetAncestor(wintypes.HWND(self.winfo_id()), 2)  # GA_ROOT
        except Exception:
            return None

    def _force_repaint(self):
        """Invalidate + erase the whole window so a resize leaves no ghost
        trails. No UPDATENOW → Windows coalesces the repaint (less flicker)."""
        try:
            import ctypes
            hwnd = self._hwnd()
            if hwnd:
                # RDW_INVALIDATE|RDW_ERASE|RDW_ALLCHILDREN
                ctypes.windll.user32.RedrawWindow(hwnd, None, None, 0x1 | 0x4 | 0x80)
        except Exception:
            pass

    def _round_corners(self, rounded=True):
        """Native Windows 11 rounded corners via DWM (antialiased, incl. border)."""
        try:
            import ctypes
            from ctypes import wintypes
            self.update_idletasks()
            u32, dwm = ctypes.windll.user32, ctypes.windll.dwmapi
            u32.SetWindowRgn.argtypes = [wintypes.HWND, wintypes.HRGN, wintypes.BOOL]
            hwnd = self._hwnd()
            if not hwnd:
                return
            u32.SetWindowRgn(hwnd, 0, True)              # drop any hard clip
            pref = ctypes.c_int(2 if rounded else 1)     # ROUND : DONOTROUND
            dwm.DwmSetWindowAttribute(hwnd, 33, ctypes.byref(pref), ctypes.sizeof(pref))
        except Exception:
            pass

    def _drag_start(self, e):
        self._dx = e.x_root - self.winfo_x()
        self._dy = e.y_root - self.winfo_y()

    def _drag_motion(self, e):
        if self._maximized:
            return
        self.geometry(f"+{e.x_root-self._dx}+{e.y_root-self._dy}")

    def _minimize(self):
        # Minimize the borderless window to the taskbar button (given by _win_taskbar's
        # WS_EX_APPWINDOW). Toggling overrideredirect + iconify created a stray floating
        # title box instead of minimizing, so drive it via the Win32 API directly.
        if IS_WIN:
            try:
                import ctypes
                hwnd = self._hwnd()
                if hwnd:
                    ctypes.windll.user32.ShowWindow(hwnd, 6)   # SW_MINIMIZE
                    return
            except Exception:
                pass
        self.iconify()

    def _on_restore(self, _=None):
        # re-assert rounded corners after the window is restored from the taskbar
        if IS_WIN:
            self.after(30, lambda: self._round_corners(not self._maximized))

    def _toggle_max(self):
        if self._maximized:
            if self._restore_geo:
                self.geometry(self._restore_geo)
            self._maximized = False
            self.after(10, lambda: self._round_corners(True))
        else:
            self._restore_geo = self.geometry()
            try:
                import ctypes
                from ctypes import wintypes
                r = wintypes.RECT()
                ctypes.windll.user32.SystemParametersInfoW(0x0030, 0, ctypes.byref(r), 0)
                self.geometry(f"{r.right-r.left}x{r.bottom-r.top}+{r.left}+{r.top}")
            except Exception:
                self.geometry(f"{self.winfo_screenwidth()}x{self.winfo_screenheight()}+0+0")
            self._maximized = True
            self.after(10, lambda: self._round_corners(False))
        self.update_idletasks()
        self._force_repaint()

    # ── manual edge / corner resizing (borderless window) ─────────
    def _make_grips(self):
        GR = 4
        cur = {"n": "sb_v_double_arrow", "s": "sb_v_double_arrow",
               "e": "sb_h_double_arrow", "w": "sb_h_double_arrow",
               "nw": "size_nw_se", "se": "size_nw_se",
               "ne": "size_ne_sw", "sw": "size_ne_sw"}

        def grip(mode, **place):
            g = tk.Frame(self, bg=BK, cursor=cur[mode])
            g.place(**place)
            g.bind("<Button-1>",  lambda e, m=mode: self._rs_start(e, m))
            g.bind("<B1-Motion>", self._rs_move)
            g.lift()

        grip("n",  x=GR, y=0,                  relwidth=1.0,  width=-2 * GR, height=GR)
        grip("s",  x=GR, rely=1.0, y=-GR,      relwidth=1.0,  width=-2 * GR, height=GR)
        grip("w",  x=0,  y=GR,                 relheight=1.0, height=-2 * GR, width=GR)
        grip("e",  relx=1.0, x=-GR, y=GR,      relheight=1.0, height=-2 * GR, width=GR)
        grip("nw", x=0, y=0,                   width=GR, height=GR)
        grip("ne", relx=1.0, x=-GR, y=0,       width=GR, height=GR)
        grip("sw", x=0, rely=1.0, y=-GR,       width=GR, height=GR)
        grip("se", relx=1.0, x=-GR, rely=1.0, y=-GR, width=GR, height=GR)

    def _rs_start(self, e, mode):
        self._rs_mode = mode
        self._rs_x, self._rs_y = e.x_root, e.y_root
        self._rs_geo = (self.winfo_x(), self.winfo_y(),
                        self.winfo_width(), self.winfo_height())

    def _rs_move(self, e):
        MINW, MINH = 560, 380
        x0, y0, w0, h0 = self._rs_geo
        dx, dy = e.x_root - self._rs_x, e.y_root - self._rs_y
        nx, ny, nw, nh = x0, y0, w0, h0
        m = self._rs_mode
        if "e" in m:
            nw = max(MINW, w0 + dx)
        if "s" in m:
            nh = max(MINH, h0 + dy)
        if "w" in m:
            nw = max(MINW, w0 - dx); nx = x0 + (w0 - nw)
        if "n" in m:
            nh = max(MINH, h0 - dy); ny = y0 + (h0 - nh)
        # coalesce rapid motion events into one geometry update per idle cycle
        self._rs_target = f"{nw}x{nh}+{nx}+{ny}"
        if not getattr(self, "_rs_pending", False):
            self._rs_pending = True
            self.after_idle(self._rs_apply)

    def _rs_apply(self):
        self._rs_pending = False
        target = getattr(self, "_rs_target", None)
        if target:
            self.geometry(target)
            self.update_idletasks()
            self._force_repaint()

    # ── UI skeleton ───────────────────────────────────────────────
    def _ui(self):
        root = tk.Frame(self, bg=BK)
        root.pack(fill="both", expand=True)

        # ── Windows-11-style title bar (custom borderless chrome; Windows only) ──
        if IS_WIN:
            tbar = tk.Frame(root, bg=TBR, height=30)
            tbar.pack(fill="x")
            tbar.pack_propagate(False)
            drag = tk.Label(tbar, bg=TBR, cursor="fleur")
            drag.pack(side="left", fill="both", expand=True)
            for w in (tbar, drag):
                w.bind("<Button-1>",        self._drag_start)
                w.bind("<B1-Motion>",       self._drag_motion)

            def cap(glyph, cmd, hbg, hfg="#ffffff"):
                b = tk.Label(tbar, text=glyph, font=("Segoe MDL2 Assets", 10),
                             bg=TBR, fg=T2, width=5, cursor="hand2")
                b.pack(side="right", fill="y")
                b.bind("<Button-1>", lambda _: cmd())
                b.bind("<Enter>", lambda _, w=b: w.configure(bg=hbg, fg=hfg))
                b.bind("<Leave>", lambda _, w=b: w.configure(bg=TBR, fg=T2))
                return b
            cap("", self._quit,       "#c42b1c")   # close
            cap("", self._minimize,   "#2a2a2a")   # minimize

        # thin banner shown only when an update is available (packed above the body)
        self._update_bar = tk.Frame(root, bg=BUB_S)

        # ── Body: sidebar + vertical divider + main ──
        self._body = body = tk.Frame(root, bg=BK)
        body.pack(fill="both", expand=True)

        # Sidebar (260px)
        self._sb_frame = tk.Frame(body, bg=SBG, width=260)
        self._sb_frame.pack(side="left", fill="y")
        self._sb_frame.pack_propagate(False)

        # Sidebar top bar: ≡ menu icon + rounded search pill
        sbar = tk.Frame(self._sb_frame, bg=SBG, height=52)
        sbar.pack(fill="x")
        sbar.pack_propagate(False)

        # Hamburger menu icon → opens info/keys panel
        _icon_widget(sbar, 20, "menu", lambda: self._show("info", self._open_info),
                     fg=T2, hover=T1, tip="My profile & keys").pack(
                     side="left", padx=(10, 4), pady=14)

        # Search pill: rounded filled field with magnifier inside (filters name + key)
        self._search_var = tk.StringVar()
        self._search_var.trace("w", lambda *_: self._filter_contacts())
        pill = tk.Canvas(sbar, height=30, bg=SBG, highlightthickness=0, bd=0)
        pill.pack(side="left", fill="x", expand=True, padx=(4, 12), pady=11)
        se = tk.Entry(pill, textvariable=self._search_var, font=FM,
                      bg=INP, fg=T1, insertbackground=T1,
                      relief="flat", bd=0, highlightthickness=0)
        self._search_entry = se
        mag_img = _render_icon("search", 16, T3, INP) if _HAS_PIL else None
        if mag_img is not None:
            pill._mag = mag_img                       # keep ref from GC
        ent_win = pill.create_window(30, 15, window=se, anchor="w")

        def _pill_redraw(_=None):
            w = pill.winfo_width()
            if w <= 1:
                return
            h = 30
            _round_bg(pill, INP, RAD_W, outline=DIV)
            pill.delete("mag")
            if mag_img is not None:
                pill.create_image(16, h // 2, image=mag_img, tags="mag")
            pill.coords(ent_win, 30, h // 2)
            pill.itemconfig(ent_win, width=w - 42, height=h - 10)
        pill.bind("<Configure>", _pill_redraw)
        pill.bind("<Button-1>", lambda _: se.focus_set())

        PH = "Search"
        def _set_ph():
            if not self._search_var.get():
                se._ph = True
                se.configure(fg=T3)
                se.insert(0, PH)
        def _clr_ph(_=None):
            if getattr(se, "_ph", False):
                se.delete(0, "end")
                se.configure(fg=T1)
                se._ph = False
        def _maybe_ph(_=None):
            if not se.get():
                _set_ph()
        se.bind("<FocusIn>", _clr_ph)
        se.bind("<FocusOut>", _maybe_ph)
        _set_ph()

        # Bottom-left toolbar: Sign/Encrypt · Decrypt/Verify · Import (.asc)
        tk.Frame(self._sb_frame, bg=DIV, height=1).pack(side="bottom", fill="x")
        tools = tk.Frame(self._sb_frame, bg=SBG)
        tools.pack(side="bottom", fill="x", pady=10)
        for kind, view, cmd, tip in (
                ("plus",    "add",     self._open_add_contact, "Add contact"),
                ("encrypt", "encrypt", self._open_encrypt,     "Sign / Encrypt"),
                ("verify",  "decrypt", self._open_decrypt,     "Decrypt / Verify"),
                ("keycard", "card",    self._open_card,        "Smartcard / Keycard")):
            _icon_widget(tools, 24, kind, lambda v=view, c=cmd: self._show(v, c),
                         fg=T2, hover=T1, tip=tip).pack(
                side="left", padx=(16, 18) if kind == "plus" else (0, 18))

        # Contact list (scrollable, fills the middle; scrollbar on the left)
        self._clist_sf = ScrollFrame(self._sb_frame, bg=SBG, thumb_side="left")
        self._clist_sf.pack(fill="both", expand=True)
        self._clist = self._clist_sf.inner

        # Vertical divider
        tk.Frame(body, bg=DIV, width=1).pack(side="left", fill="y")

        # Main panel: a host containing the current inner panel frame
        self._main_host = tk.Frame(body, bg=BK)
        self._main_host.pack(side="left", fill="both", expand=True)
        self._main = tk.Frame(self._main_host, bg=BK)
        self._main.place(x=0, y=0, relwidth=1, relheight=1)

        self._welcome()
        self._rebuild_list()

    def _show(self, view_id, builder):
        """Navigate to a tab. If it is already the one on screen, do nothing — so
        re-clicking the current tab never reloads it (and never loses typed text).
        Internal refreshes call the builders directly to force a rebuild."""
        if view_id == getattr(self, "_view", None):
            return
        builder()

    def _clear_main(self):
        # Build the new panel OFF-SCREEN (old stays fully visible meanwhile), then
        # reveal it in one step and drop the old — instant, no animation, no flicker.
        self._clear_drop_highlight()      # old field is about to be destroyed
        self._drop_targets = []           # re-registered by the incoming panel
        self._view = None                 # a builder will tag the new panel
        self._main = tk.Frame(self._main_host, bg=BK)
        self._main.place(relx=1.0, y=0, relwidth=1, relheight=1)
        self.after_idle(self._swap_in)

    def _swap_in(self):
        self._main.place_configure(relx=0)      # reveal the fully-built panel
        self._main.lift()
        for w in self._main_host.winfo_children():   # drop any older panels
            if w is not self._main:
                try: w.destroy()
                except Exception: pass

    # ── Panel header ──────────────────────────────────────────────
    def _panel_hdr(self, title, back_fn=None, right_widgets_fn=None):
        h = tk.Frame(self._main, bg=HDR, height=52)
        h.pack(fill="x")
        h.pack_propagate(False)
        if back_fn:
            _icon_widget(h, 16, "back", back_fn, fg=T3, hover=T1).pack(
                side="left", padx=(14, 0), pady=16)
        tk.Label(h, text=title, font=FB, bg=HDR, fg=T1,
                 padx=14 if not back_fn else 8).pack(side="left", pady=10)
        if right_widgets_fn:
            right_widgets_fn(h)
        tk.Frame(self._main, bg=DIV, height=1).pack(fill="x")

    # ── Welcome ───────────────────────────────────────────────────
    def _welcome(self):
        self._clear_main()
        self._view = "welcome"
        f = tk.Frame(self._main, bg=BK)
        f.place(relx=0.5, rely=0.45, anchor="center")
        vl = tk.Label(f, text=f" PGPM v{APP_VERSION}", font=FS, bg=BK, fg=T3)
        auth = self._asset_img("authority.png", 14, tint=T3)
        if auth is not None:
            vl.configure(image=auth, compound="left")
            vl._img = auth                       # keep a ref from GC
        vl.pack()

    # ── Info / Keys panel (≡ hamburger) ───────────────────────────
    def _open_info(self):
        self._clear_main()
        self._view = "info"
        self._panel_hdr("Profile & Keys", back_fn=self._welcome)

        scroll = ScrollFrame(self._main, bg=BK)
        scroll.pack(fill="both", expand=True)
        f = scroll.inner

        # Status badge
        st_row = tk.Frame(f, bg=BK)
        st_row.pack(fill="x", padx=20, pady=(16, 0))
        if not Card.available():
            stc, stt = RD, "● GnuPG not found — install Gpg4win for key storage"
        elif self._secret:
            stc, stt = GN, f"● {len(self._secret)} of your keys in the GnuPG keyring"
        else:
            stc, stt = RD, "● no private keys yet — generate or import one below"
        tk.Label(st_row, text=stt, font=(UI, 9, "bold"),
                 bg=BK, fg=stc).pack(anchor="w")

        def _section(title):
            tk.Frame(f, bg=DIV, height=1).pack(fill="x", padx=20, pady=(16, 0))
            tk.Label(f, text=title, font=(UI, 8, "bold"),
                     bg=BK, fg=T3).pack(anchor="w", padx=20, pady=(6, 4))

        # ─ My keys (compact cards; click one to make it active) ─
        # Cards flow left-to-right and wrap to the next line to save space.
        self._key_cards = []
        if self._secret:
            tk.Frame(f, bg=BK, height=8).pack()
            kwrap = tk.Frame(f, bg=BK); kwrap.pack(anchor="w", fill="x", padx=20)
            cards, maxw = [], 0
            for k in self._secret:
                cv, w = self._key_row(kwrap, k)
                cards.append(cv); maxw = max(maxw, w)
            self._key_cards = cards
            gap = 10

            def _reflow(_=None):
                width = kwrap.winfo_width()
                if width <= 1:
                    return
                cols = max(1, (width + gap) // (maxw + gap))
                for i, cv in enumerate(cards):
                    cv.grid(row=i // cols, column=i % cols,
                            padx=(0, gap), pady=(0, 6), sticky="w")
            kwrap.bind("<Configure>", _reflow)
            self.after(0, _reflow)

        # ─ Active key details + actions (refreshed in place on switch) ─
        self._akd = tk.Frame(f, bg=BK)
        self._akd.pack(fill="x")
        self._fill_active_details()

        # ─ Generate new key pair ─
        _section("GENERATE NEW KEY PAIR")
        gr = tk.Frame(f, bg=BK)
        gr.pack(fill="x", padx=20, pady=(4, 0))

        tk.Label(gr, text="name", font=FS, bg=BK, fg=T3).pack(anchor="w")
        self._gen_name = tk.StringVar()
        gne = _make_entry(gr, self._gen_name)

        tk.Label(gr, text="email  (optional)", font=FS, bg=BK, fg=T3).pack(
            anchor="w", pady=(6, 0))
        self._gen_email = tk.StringVar()
        _make_entry(gr, self._gen_email)

        tk.Label(gr, text="password  (optional — protects the key in gpg)",
                 font=FS, bg=BK, fg=T3).pack(anchor="w", pady=(6, 0))
        self._gen_pass = tk.StringVar()
        _make_entry(gr, self._gen_pass, show="•")

        self._gen_btn = _make_btn(gr, "generate  (rsa 4096)", self._generate_key, w=200)
        self._gen_btn.pack(anchor="w", pady=(10, 0))

        # ─ Import key (public or private, armored) ─
        _section("IMPORT KEY  ·  public or private, armored")
        ir = tk.Frame(f, bg=BK)
        ir.pack(fill="x", padx=20, pady=(4, 0))

        icv, self._imp_t = _round_text(ir, height_px=88, fg=T1)
        icv.pack(fill="x", pady=(2, 8))
        scroll.bind_scroll_to(self._imp_t)

        ibr = tk.Frame(ir, bg=BK); ibr.pack(anchor="w")
        _make_btn(ibr, "import pasted",    self._import_key, w=130).pack(side="left")
        _make_btn(ibr, "import .asc file", self._import_asc, w=140).pack(side="left", padx=(8, 0))

        tk.Frame(f, bg=BK, height=20).pack()
        scroll.scroll_top()

    def _key_row(self, parent, k):
        # compact card: a small rounded box hugging just the icon + name + meta.
        # Builds the canvas but does NOT place it — caller arranges (flow layout).
        H = 50
        nm = k.get("name") or k.get("keyid", "")
        meta = "   ".join(x for x in (k.get("email", ""), k.get("keyid", "")) if x)
        kind = "keycard" if k.get("on_card") else "key"
        icon = _render_icon(kind, 20, T2) if _HAS_PIL else None

        cv = tk.Canvas(parent, height=H, bg=BK, highlightthickness=0, bd=0, cursor="hand2")
        cv._fpr = k["fpr"]
        # measure text so the box wraps just around the content
        nid = cv.create_text(44, 17, text=nm, anchor="w", font=FB)
        mid = cv.create_text(44, 34, text=meta, anchor="w", font=(UI, 8))
        cw = max(cv.bbox(nid)[2], cv.bbox(mid)[2]) + 14
        cv.delete("all")
        cv.configure(width=cw)

        def redraw(_=None):
            if cv.winfo_width() <= 1:
                return
            # the active key gets a thin white border so it's clear which is in use
            outline = WT if cv._fpr == self.my_fpr else DIV
            _round_bg(cv, INP, 13, outline=outline)
            cv.delete("fg")
            if icon is not None:
                cv._kicon = icon
                cv.create_image(23, H // 2, image=icon, tags="fg")
            cv.create_text(44, 17, text=nm, anchor="w", font=FB, fill=T1, tags="fg")
            cv.create_text(44, 34, text=meta, anchor="w", font=(UI, 8), fill=T3, tags="fg")
        cv._redraw = redraw
        cv.bind("<Configure>", redraw)
        cv.bind("<Button-1>", lambda _, fp=k["fpr"]: self._activate_key(fp))
        return cv, cw

    def _fill_active_details(self):
        """Show the ACTIVE KEY section. Built once per panel; switching keys then
        updates the values in place — no widget destroy/recreate, so no flicker
        (same idiom as _highlight_active for the sidebar)."""
        akd = getattr(self, "_akd", None)
        if akd is None or not akd.winfo_exists():
            return
        ak = self._active_key()
        if not ak:
            for w in akd.winfo_children():
                w.destroy()
            self._akd_vals = None
            return

        # Fast path: the section already exists in THIS akd frame → just refresh
        # the text + actions in place. This is what makes key-switching flicker-free.
        if getattr(self, "_akd_vals", None) and getattr(self, "_akd_owner", None) is akd:
            self._update_active_details(ak)
            return

        # First build for this panel (akd was freshly created by _open_info).
        for w in akd.winfo_children():
            w.destroy()
        self._akd_owner = akd
        self._akd_vals = {}
        self._akd_oncard = None      # force the action row to build on first update

        tk.Frame(akd, bg=DIV, height=1).pack(fill="x", padx=20, pady=(16, 0))
        tk.Label(akd, text="ACTIVE KEY", font=(UI, 8, "bold"),
                 bg=BK, fg=T3).pack(anchor="w", padx=20, pady=(6, 4))
        info = tk.Frame(akd, bg=BK); info.pack(fill="x", padx=20)
        for label in ("Name", "Email", "Key ID", "Fingerprint",
                      "Created", "Valid until", "Storage"):
            self._akd_vals[label] = _kv_row(info, label, "")
        self._akd_act = tk.Frame(akd, bg=BK)
        self._akd_act.pack(anchor="w", padx=20, pady=(10, 0))
        self._update_active_details(ak)

    def _update_active_details(self, ak):
        """Refresh the ACTIVE KEY values + action links in place (no rebuild)."""
        def setv(key, val):
            e = self._akd_vals.get(key)
            if e is None:
                return
            e.configure(state="normal")
            e.delete(0, "end")
            e.insert(0, val)
            e.configure(state="readonly")          # stays selectable/copyable
        setv("Name",        ak.get("name", "") or "—")
        setv("Email",       ak.get("email", "") or "—")
        setv("Key ID",      ak.get("keyid", "") or "—")
        setv("Fingerprint", ak.get("fpr", "") or "—")
        setv("Created",     ak["created"].strftime("%Y-%m-%d") if ak.get("created") else "—")
        exp = ak.get("expires")
        vu = exp.strftime("%Y-%m-%d") if exp else "unlimited"
        st = self._key_status(ak)
        if st:
            vu += f"  ({st[0]})"                # expired / expires soon
        setv("Valid until", vu)
        setv("Storage",     "smartcard" if ak.get("on_card") else "this computer")

        # Action links read the live active fpr, so switching between two software
        # keys leaves them untouched. Only rebuild when on-card state changes.
        self._akd_fpr = ak["fpr"]
        oncard = bool(ak.get("on_card"))
        if oncard == getattr(self, "_akd_oncard", None):
            return
        self._akd_oncard = oncard
        act = self._akd_act
        for w in act.winfo_children():
            w.destroy()
        sep = lambda: tk.Label(act, text=" · ", font=FS, bg=BK, fg=T4).pack(side="left")
        _link(act, "copy public key", lambda: self._copy_pub(self._akd_fpr)).pack(side="left")
        if not oncard:
            sep(); _link(act, "copy private key", lambda: self._copy_priv(self._akd_fpr)).pack(side="left")
        sep(); _link(act, "export public", lambda: self._export_key(self._akd_fpr, False)).pack(side="left")
        if not oncard:
            sep(); _link(act, "export private", lambda: self._export_key(self._akd_fpr, True)).pack(side="left")
        sep(); _link(act, "delete", lambda: self._delete_key(self._akd_fpr), fg=RD).pack(side="left")

    def _copy_priv(self, fpr):
        try:
            sk = Card.export_secret(fpr)
        except Exception as e:
            messagebox.showerror("Error", str(e), parent=self); return
        if not sk:
            messagebox.showwarning("Nothing to copy", "No private key found.", parent=self); return
        self.clipboard_clear(); self.clipboard_append(sk)
        messagebox.showinfo("Copied", "Private key copied to clipboard.", parent=self)

    def _copy_pub(self, fpr):
        try:
            pub = Card.export_pub(fpr)
        except Exception as e:
            messagebox.showerror("Error", str(e), parent=self); return
        if not pub:
            messagebox.showwarning("Nothing to copy", "No public key found.", parent=self); return
        self.clipboard_clear(); self.clipboard_append(pub)
        messagebox.showinfo("Copied", "Public key copied to clipboard.", parent=self)

    def _activate_key(self, fpr):
        """Switch the active key; refresh the ACTIVE KEY section in place (no page rebuild)."""
        if fpr == self.my_fpr:
            return
        self._set_active(fpr)
        self._fill_active_details()
        self._refresh_key_borders()

    def _refresh_key_borders(self):
        """Re-tint the My-keys card borders in place so the active one is outlined —
        no list rebuild, no flicker (same idiom as _highlight_active)."""
        for cv in getattr(self, "_key_cards", []):
            try:
                if cv.winfo_exists():
                    cv._redraw()
            except Exception:
                pass

    def _export_key(self, fpr, secret):
        try:
            data = Card.export_secret(fpr) if secret else Card.export_pub(fpr)
        except Exception as e:
            messagebox.showerror("Export error", str(e), parent=self); return
        if not data:
            messagebox.showwarning("Export", "Nothing to export.", parent=self); return
        out = filedialog.asksaveasfilename(
            parent=self, title="Save key", defaultextension=".asc",
            initialfile=("secret" if secret else "public") + ".asc",
            filetypes=[("ASC files", "*.asc"), ("All files", "*.*")])
        if not out:
            return
        try:
            with open(out, "w", encoding="utf-8") as fh:
                fh.write(data)
        except Exception as e:
            messagebox.showerror("Save error", str(e), parent=self); return
        messagebox.showinfo("Exported", "Key saved.", parent=self)

    def _delete_key(self, fpr):
        k = next((x for x in self._secret if x["fpr"] == fpr), None)
        nm = (k.get("name") if k else "") or fpr[:16]
        if not messagebox.askyesno("Delete key",
            f"Delete {nm} (including its private key) from the GnuPG keyring?\n"
            "This cannot be undone.", parent=self):
            return
        try:
            Card.delete_secret(fpr)
        except Exception as e:
            messagebox.showerror("Error", str(e), parent=self); return
        self._refresh()
        self._open_info()

    def _generate_key(self):
        name = self._gen_name.get().strip()
        if not name:
            messagebox.showwarning("Required", "Name is required.", parent=self); return
        if not Card.available():
            messagebox.showerror("No GnuPG", "GnuPG is required. Install Gpg4win.",
                                 parent=self); return
        self._gen_btn.set_text("generating…"); self._gen_btn.set_enabled(False)
        self.update()
        email, pw = self._gen_email.get().strip(), self._gen_pass.get()
        err = [None]

        def worker():
            try:
                Card.generate(name, email, pw)
            except Exception as e:
                err[0] = e

        t = threading.Thread(target=worker, daemon=True)
        t.start()

        def poll():
            if t.is_alive():
                self.after(200, poll); return
            if err[0]:
                messagebox.showerror("Error", str(err[0]), parent=self)
                self._gen_btn.set_text("generate  (rsa 4096)"); self._gen_btn.set_enabled(True)
            else:
                self._reload_keys()
                for k in self._secret:           # make the new key active
                    if k.get("name") == name:
                        self.my_fpr = k["fpr"]; self._save_config(); break
                self._rebuild_list()
                self._open_info()

        self.after(200, poll)

    def _import_key(self):
        raw = self._imp_t.get("1.0", "end-1c").strip()
        if not raw or "BEGIN PGP" not in raw:
            messagebox.showwarning("Required", "Paste an armored PGP key first.", parent=self); return
        try:
            Card.import_key(raw)
        except Exception as e:
            messagebox.showerror("Import Error", str(e), parent=self); return
        self._refresh()
        messagebox.showinfo("Imported", "Key imported into the GnuPG keyring.", parent=self)
        self._open_info()

    # ── keyring state (GnuPG is the source of truth, like Kleopatra) ─
    def _migrate_legacy(self):
        cfg = _load(_CONFIG, {})
        if cfg.get("migrated") or not Card.available():
            return
        try:
            kd = _load(_KEYS, {})
            for blob in (kd.get("private"), kd.get("public")):
                if blob and "BEGIN PGP" in blob:
                    try: Card.import_key(blob)
                    except Exception: pass
            for c in _load(_CONTACTS, []):
                if c.get("public_key") and "BEGIN PGP" in c["public_key"]:
                    try: Card.import_key(c["public_key"])
                    except Exception: pass
        except Exception:
            pass
        cfg["migrated"] = True
        _save(_CONFIG, cfg)

    def _reload_keys(self):
        avail = Card.available()
        secret = Card.list_secret() if avail else []
        contacts = Card.list_public() if avail else []
        # A keycard leaves a "stub" secret key in the keyring even after it is
        # unplugged. Hide the on-card keys (and their matching public entries) UNLESS a
        # card is confirmed present, so a keycard only shows while it is plugged in.
        # (None = not yet probed → hide until the watcher confirms a card is present.)
        if getattr(self, "_card_present", None) is not True:
            gone = {k["fpr"] for k in secret if k.get("on_card")}
            if gone:
                secret = [k for k in secret if not k.get("on_card")]
                contacts = [c for c in contacts if c.get("fpr") not in gone]
        # your own keys are your identities, not contacts — hide them from the list
        own = {k["fpr"] for k in secret}
        contacts = [c for c in contacts if c.get("fpr") not in own]
        # honour the user's manual drag-ordering (saved fpr order); keys not in the
        # saved order keep their gpg order and fall to the bottom (stable sort)
        order = self._cfg_get("contact_order", []) or []
        if order:
            pos = {f: i for i, f in enumerate(order)}
            big = len(order)
            contacts.sort(key=lambda c: pos.get(c.get("fpr"), big))
        self._secret = secret
        self.contacts = contacts
        _log(f"reload_keys: gpg={Card.gpg()!r} avail={avail} secret={len(secret)} "
             f"contacts={len(contacts)} card_present={getattr(self,'_card_present',None)}")
        sfprs = {k["fpr"] for k in self._secret}
        if self.my_fpr not in sfprs:
            self.my_fpr = self._secret[0]["fpr"] if self._secret else None
            self._save_config()

    def _save_config(self):
        cfg = _load(_CONFIG, {})
        cfg["my_fpr"] = self.my_fpr
        _save(_CONFIG, cfg)

    def _cfg_get(self, key, default=None):
        return _load(_CONFIG, {}).get(key, default)

    def _cfg_set(self, key, value):
        cfg = _load(_CONFIG, {})
        cfg[key] = value
        _save(_CONFIG, cfg)

    def _asset_img(self, name, size, tint=None):
        """Load a small bundled PNG (e.g. valid.png) as a cached PhotoImage.
        If `tint` (a #rrggbb) is given, recolour the opaque region to it (keeps the
        transparent parts) so the badge matches the surrounding text colour."""
        cache = getattr(self, "_asset_cache", None)
        if cache is None:
            cache = self._asset_cache = {}
        key = (name, size, tint)
        if key in cache:
            return cache[key]
        img = None
        if _HAS_PIL:
            for p in (_resource(name), os.path.join(_DESKTOP, "128x128", name)):
                if p and os.path.exists(p):
                    try:
                        im = Image.open(p).convert("RGBA")
                        im.thumbnail((size, size), Image.LANCZOS)
                        if tint:
                            rgb = tuple(int(tint[i:i + 2], 16) for i in (1, 3, 5))
                            solid = Image.new("RGBA", im.size, rgb + (0,))
                            solid.putalpha(im.getchannel("A"))
                            im = solid
                        img = ImageTk.PhotoImage(im)
                        break
                    except Exception:
                        img = None
        cache[key] = img
        return img

    @staticmethod
    def _key_status(k):
        """(badge_text, colour) for an expired / expiring-soon key, else None."""
        exp = k.get("expires")
        if not exp:
            return None
        try:
            now = datetime.datetime.now(exp.tzinfo) if getattr(exp, "tzinfo", None) \
                else datetime.datetime.now()
            if exp < now:
                return ("expired", RD)
            if (exp - now).days <= 30:
                return ("expires soon", AM)
        except Exception:
            return None
        return None

    def _set_active(self, fpr):
        self.my_fpr = fpr
        self._save_config()

    def _active_key(self):
        for k in self._secret:
            if k["fpr"] == self.my_fpr:
                return k
        return None

    def _has_key(self):
        return bool(self.my_fpr)

    # ── sign / decrypt (all via gpg; PIN/passphrase via gpg pinentry) ─
    def _sign(self, text):
        return Card.sign(text, self.my_fpr)

    def _decrypt(self, armored, known_pubs=()):
        return Card.decrypt(armored)

    def _refresh(self):
        """Reload keys from gpg and rebuild the contact list."""
        self._reload_keys()
        self._rebuild_list()

    # ── live keycard watch (plug / unplug detection) ──────────────
    def _start_card_watch(self):
        """Poll for keycard presence so plugging/unplugging updates the UI live."""
        if not Card.available():
            return
        self._card_watch()

    def _card_watch(self):
        # probe gpg in a worker (uses Card._NOWIN → no console flash on Windows); a
        # main-thread poll picks up the result so Tk is never touched off-thread
        # (same idiom as _card_action / _generate_key).
        holder = {"done": False, "present": False}

        def worker():
            try:
                holder["present"] = bool(Card.status().get("present"))
            except Exception:
                holder["present"] = False
            holder["done"] = True

        threading.Thread(target=worker, daemon=True).start()
        self._card_watch_poll(holder)

    def _card_watch_poll(self, holder):
        if not self.winfo_exists():
            return
        if not holder["done"]:
            self.after(150, lambda: self._card_watch_poll(holder))
            return
        if holder["present"] != getattr(self, "_card_present", None):
            self._card_present = holder["present"]
            self._reload_keys()
            self._on_keys_changed()
        self._card_watch_job = self.after(2500, self._card_watch)

    def _on_keys_changed(self):
        """A keycard was plugged/unplugged: refresh the visible UI in place."""
        try:
            self._rebuild_list()
        except Exception:
            pass
        if getattr(self, "_view", None) == "info":
            self._open_info()        # flicker-free off-screen swap

    # ── Smartcard / Keycard panel (Nitrokey · YubiKey via GnuPG) ──
    def _open_card(self):
        self._clear_main()
        self._view = "card"
        self._panel_hdr("Smartcard / Keycard", back_fn=self._welcome)
        scroll = ScrollFrame(self._main, bg=BK)
        scroll.pack(fill="both", expand=True)
        f = scroll.inner

        if not Card.available():
            tk.Label(f, text="● GnuPG is required for hardware keycards",
                     font=(UI, 9, "bold"), bg=BK, fg=RD).pack(anchor="w", padx=20, pady=(18, 4))
            tk.Label(f, text="Install Gpg4win, then plug in your Nitrokey or YubiKey.",
                     font=FS, bg=BK, fg=T2).pack(anchor="w", padx=20)
            tk.Label(f, text="https://www.gpg4win.org", font=FMO,
                     bg=BK, fg=T3).pack(anchor="w", padx=20, pady=(2, 0))
            return

        tk.Label(f, text="Use a Nitrokey / YubiKey OpenPGP card via GnuPG. The private key stays "
                         "on the device; signing and decryption happen on-card (PIN via GnuPG's "
                         "pinentry dialog).",
                 font=FS, bg=BK, fg=T2, wraplength=520, justify="left").pack(
                 anchor="w", padx=20, pady=(18, 0))

        body = tk.Frame(f, bg=BK)
        body.pack(fill="x", padx=20, pady=(10, 0))

        def actbar(parent):
            r = tk.Frame(parent, bg=BK); r.pack(anchor="w", pady=(8, 0)); return r

        def detect():
            # show "detecting…" then probe gpg in a worker thread, so the UI thread
            # never freezes (a blocking Card.status() left a ghost of this button).
            for w in body.winfo_children():
                w.destroy()
            tk.Label(body, text="detecting…", font=FS, bg=BK, fg=T3).pack(anchor="w")
            res = [None]

            def worker():
                try: res[0] = Card.status()
                except Exception: res[0] = {"present": False}

            t = threading.Thread(target=worker, daemon=True)
            t.start()

            def poll():
                if not body.winfo_exists():
                    return                       # panel was navigated away
                if t.is_alive():
                    self.after(150, poll); return
                render(res[0] or {"present": False})
            self.after(150, poll)

        def render(info):
            for w in body.winfo_children():
                w.destroy()
            if not info.get("present"):
                tk.Label(body, text="No OpenPGP card detected.", font=(UI, 9, "bold"),
                         bg=BK, fg=RD).pack(anchor="w")
                tk.Label(body, text="Insert your Nitrokey / YubiKey and click Detect again.",
                         font=FS, bg=BK, fg=T2).pack(anchor="w", pady=(2, 0))
                return
            for lbl, key in (("Cardholder", "name"), ("Serial", "serial"),
                             ("Version", "version"), ("Reader", "reader"),
                             ("Signature key", "sig_fpr"), ("Encryption key", "enc_fpr"),
                             ("Auth key", "auth_fpr"), ("PIN retries", "pin_retries")):
                if info.get(key):
                    _kv_row(body, lbl, info[key])
            enc = info.get("enc_fpr") or info.get("sig_fpr")

            def use_card(fpr=enc):
                if not fpr:
                    messagebox.showwarning("No key", "Card has no key.", parent=self); return
                if not Card.export_pub(fpr):
                    messagebox.showwarning("Public key not in keyring",
                        "GnuPG doesn't have this card's public key.\n"
                        "Set a key URL on the card and Fetch, or import the matching .asc.",
                        parent=self); return
                self._reload_keys()
                self.my_fpr = fpr; self._save_config(); self._rebuild_list()
                messagebox.showinfo("Keycard active",
                    "This card's key is now your active signing/decryption key.", parent=self)
                self._open_card()

            r1 = actbar(body)
            _make_btn(r1, "Use this card", use_card, w=130, pbg=BK, bold=True).pack(side="left")
            _make_btn(r1, "Move key to card", self._card_move_key, w=150, pbg=BK).pack(
                side="left", padx=(8, 0))
            r2 = actbar(body)
            _make_btn(r2, "Change PIN", lambda: self._card_action(
                lambda: Card.card_change_pin("1"), "PIN updated."), w=110, pbg=BK).pack(side="left")
            _make_btn(r2, "Admin PIN", lambda: self._card_action(
                lambda: Card.card_change_pin("3"), "Admin PIN updated."), w=104, pbg=BK).pack(
                side="left", padx=(8, 0))
            _make_btn(r2, "Unblock PIN", lambda: self._card_action(
                lambda: Card.card_change_pin("2"), "PIN unblocked."), w=114, pbg=BK).pack(
                side="left", padx=(8, 0))
            r3 = actbar(body)
            _make_btn(r3, "Set name", self._card_name, w=100, pbg=BK).pack(side="left")
            _make_btn(r3, "Set key URL", self._card_url, w=110, pbg=BK).pack(side="left", padx=(8, 0))
            _make_btn(r3, "Fetch key", lambda: self._card_action(
                lambda: Card.card_fetch(), "Fetched public key from URL."), w=100, pbg=BK).pack(
                side="left", padx=(8, 0))

        tk.Frame(f, bg=BK, height=6).pack()
        _make_btn(f, "Detect card", detect, w=140, pbg=BK, bold=True).pack(
            anchor="w", padx=20, pady=(6, 0))
        tk.Frame(f, bg=BK, height=16).pack()
        scroll.scroll_top()

    def _card_action(self, fn, ok_msg):
        """Run a card command (which may pop gpg's pinentry) off the UI thread."""
        res, err = [None], [None]

        def worker():
            try: res[0] = fn()
            except Exception as e: err[0] = e

        t = threading.Thread(target=worker, daemon=True); t.start()

        def poll():
            if t.is_alive():
                self.after(150, poll); return
            if err[0]:
                messagebox.showerror("Card error", str(err[0]), parent=self)
            else:
                r = res[0]
                rc = getattr(r, "returncode", 0)
                out = ((getattr(r, "stderr", "") or "") + (getattr(r, "stdout", "") or "")).strip()
                if rc not in (0, None) and ("error" in out.lower() or "failed" in out.lower()):
                    messagebox.showerror("Card error", out[:600] or "command failed", parent=self)
                else:
                    messagebox.showinfo("Done", ok_msg, parent=self)
                self._reload_keys(); self._rebuild_list()
            self._open_card()

        self.after(150, poll)

    def _card_name(self):
        from tkinter import simpledialog
        sn = simpledialog.askstring("Cardholder", "Surname:", parent=self)
        if sn is None:
            return
        gn = simpledialog.askstring("Cardholder", "Given name:", parent=self)
        if gn is None:
            return
        self._card_action(lambda: Card.card_set_name(sn, gn), "Cardholder name set.")

    def _card_url(self):
        from tkinter import simpledialog
        url = simpledialog.askstring("Public-key URL", "URL where your public key is published:",
                                     parent=self)
        if not url:
            return
        self._card_action(lambda: Card.card_set_url(url), "Public-key URL set.")

    def _card_move_key(self):
        soft = [k for k in self._secret if not k.get("on_card")]
        if not soft:
            messagebox.showwarning("No key",
                "No software secret key to move. Generate or import one first.", parent=self); return
        self._choose_secret("Move which key to the card?", lambda k: self._card_action(
            lambda: Card.keytocard(k["fpr"], "2"),
            "Encryption key moved to the card. (Re-detect to confirm.)"))

    def _choose_secret(self, title, on_pick):
        self._clear_main()
        self._panel_hdr(title, back_fn=self._open_card)
        f = tk.Frame(self._main, bg=BK)
        f.pack(fill="both", expand=True, padx=24, pady=16)
        for k in self._secret:
            if k.get("on_card"):
                continue
            nm = k.get("name") or k.get("keyid", "")
            _make_btn(f, nm, lambda kk=k: on_pick(kk), w=260, pbg=BK).pack(anchor="w", pady=3)

    # ── Add Contact panel ─────────────────────────────────────────
    def _open_add_contact(self):
        self._clear_main()
        self._view = "add"
        self._panel_hdr("Import key", back_fn=self._welcome)

        f = tk.Frame(self._main, bg=BK)
        f.pack(fill="both", expand=True, padx=28, pady=16)
        tk.Label(f, text="Paste a PGP public or private key (armored), or import a .asc file.\n"
                         "Keys are added to the GnuPG keyring.",
                 font=FS, bg=BK, fg=T3, justify="left").pack(anchor="w", pady=(0, 8))

        bf = tk.Frame(f, bg=BK)
        bf.pack(side="bottom", fill="x", pady=(14, 0))
        _make_btn(bf, "Import",           self._save_contact, w=110).pack(side="left")
        _make_btn(bf, "import .asc file",  self._import_asc,   w=140).pack(side="left", padx=(8, 0))
        _make_btn(bf, "Cancel",            self._welcome,      w=90).pack(side="left", padx=(8, 0))

        kcv, self._ac_key = _round_text(f, fg=T1)
        kcv.pack(fill="both", expand=True, pady=(2, 0))
        PH = "paste pgp key here…"
        self._ac_key.insert("1.0", PH); self._ac_key.configure(fg=T3)

        def _kfi(_):
            if self._ac_key.get("1.0", "end-1c") == PH:
                self._ac_key.delete("1.0", "end"); self._ac_key.configure(fg=T1)

        def _kfo(_):
            if not self._ac_key.get("1.0", "end-1c").strip():
                self._ac_key.insert("1.0", PH); self._ac_key.configure(fg=T3)

        self._ac_key.bind("<FocusIn>",  _kfi)
        self._ac_key.bind("<FocusOut>", _kfo)

    def _save_contact(self):
        PH = "paste pgp key here…"
        raw = self._ac_key.get("1.0", "end-1c").strip()
        _log(f"save_contact: raw_len={len(raw)} has_begin={'BEGIN PGP' in raw}")
        if not raw or raw == PH or "BEGIN PGP" not in raw:
            messagebox.showwarning("Required", "Paste an armored PGP key.", parent=self); return
        try:
            out = Card.import_key(raw)
            _log(f"save_contact: import out={out[:120]!r}")
        except Exception as e:
            _log(f"save_contact: import EXC {e}")
            messagebox.showerror("Import error", str(e), parent=self); return
        self._refresh()
        messagebox.showinfo("Imported", "Key imported into the keyring.", parent=self)
        self._welcome()

    # ── .asc file operations ──────────────────────────────────────
    _ASC_TYPES = [("PGP / ASC files", "*.asc *.gpg *.pgp *.txt"),
                  ("All files", "*.*")]

    def _import_asc(self):
        path = filedialog.askopenfilename(
            parent=self, title="Import key (.asc)", filetypes=self._ASC_TYPES)
        if not path:
            return
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as fh:
                raw = fh.read().strip()
        except Exception as e:
            messagebox.showerror("Read Error", str(e), parent=self); return
        if not raw or "BEGIN PGP" not in raw:
            messagebox.showerror("Not a Key",
                "That file doesn't contain a PGP key block.", parent=self); return
        try:
            Card.import_key(raw)
        except Exception as e:
            messagebox.showerror("Import error", str(e), parent=self); return
        self._refresh()
        messagebox.showinfo("Imported", "Key imported into the GnuPG keyring.", parent=self)
        self._welcome()

    def _recipients(self):
        """label -> fingerprint map of public keys in the keyring."""
        opts = {}
        if self.my_fpr:                          # encrypt a copy only you can read
            opts["Only me"] = self.my_fpr
        for c in self.contacts:
            label = c.get("name") or c.get("email") or c.get("keyid", "")
            if c.get("secret"):
                label += "  (me)"
            if label and label not in opts:
                opts[label] = c["fpr"]
        return opts

    def _load_into(self, textw):
        path = filedialog.askopenfilename(
            parent=self, title="Import file",
            filetypes=[("All files", "*.*")] + self._ASC_TYPES)
        if path:
            self._load_file_into(textw, path)

    # ── Sign / Encrypt panel ──────────────────────────────────────
    def _open_encrypt(self):
        self._clear_main()
        self._view = "encrypt"
        self._panel_hdr("Sign / Encrypt", back_fn=self._welcome)
        f = tk.Frame(self._main, bg=BK)
        f.pack(fill="both", expand=True, padx=16, pady=12)

        tcv, txt = _round_text(f, font=FM, fg=T1, pbg=BK)
        tcv.pack(fill="both", expand=True)
        self._active_textbox = txt          # import button targets this box
        self._register_drop_target(tcv, txt)   # dropped files land here
        PH = "Type or load a message…"
        txt.insert("1.0", PH); txt.configure(fg=T3)
        txt.bind("<FocusIn>", lambda _: (txt.get("1.0", "end-1c") == PH) and
                 (txt.delete("1.0", "end"), txt.configure(fg=T1)))

        bar = tk.Frame(f, bg=BK)
        bar.pack(fill="x", pady=(10, 0))
        opts = self._recipients()
        labels = list(opts.keys()) or ["(no keys)"]
        dd = _dropdown(bar, labels, default=labels[0], pbg=BK, width=130)
        tk.Label(bar, text="to", font=FS, bg=BK, fg=T3).pack(side="left", padx=(0, 5))
        dd.pack(side="left", padx=(0, 8))

        def body():
            t = txt.get("1.0", "end-1c")
            return "" if t == PH else t

        def do_sign():
            if not self._has_key():
                messagebox.showwarning("No Keys",
                    "You have no secret key. Generate or import one first.", parent=self)
                return self._open_info()
            t = body()
            if not t.strip():
                return
            try:
                out = self._sign(t)
            except Exception as e:
                return messagebox.showerror("Sign Error", str(e), parent=self)
            txt.delete("1.0", "end"); txt.insert("1.0", out); txt.configure(fg=T1)

        def do_encrypt():
            t = body()
            if not t.strip():
                return
            fpr = opts.get(dd._var.get())
            if not fpr:
                return messagebox.showwarning("No Recipient",
                    "Pick a recipient (import a key first).", parent=self)
            recips = [fpr]
            if self.my_fpr and self.my_fpr not in recips:
                recips.append(self.my_fpr)               # also encrypt to me
            try:
                out = Card.encrypt(t, recips)            # no signing
            except Exception as e:
                return messagebox.showerror("Encrypt Error", str(e), parent=self)
            txt.delete("1.0", "end"); txt.insert("1.0", out); txt.configure(fg=T1)

        def save_out():
            t = body()
            if not t.strip():
                return
            out = filedialog.asksaveasfilename(
                parent=self, title="Save", defaultextension=".asc",
                filetypes=[("ASC files", "*.asc"), ("All files", "*.*")])
            if out:
                try:
                    open(out, "w", encoding="utf-8").write(t)
                    messagebox.showinfo("Saved", "File saved.", parent=self)
                except Exception as e:
                    messagebox.showerror("Save Error", str(e), parent=self)

        _make_btn(bar, "import file", lambda: self._load_into(txt), w=110, pbg=BK).pack(side="left")
        _make_btn(bar, "save",    save_out,   w=64, pbg=BK).pack(side="right")
        _make_btn(bar, "encrypt", do_encrypt, w=86, pbg=BK).pack(side="right", padx=(0, 6))
        _make_btn(bar, "sign",    do_sign,    w=64, pbg=BK).pack(side="right", padx=(0, 6))

    # ── Decrypt / Verify panel ────────────────────────────────────
    def _open_decrypt(self):
        self._clear_main()
        self._view = "decrypt"
        self._panel_hdr("Decrypt / Verify", back_fn=self._welcome)
        f = tk.Frame(self._main, bg=BK)
        f.pack(fill="both", expand=True, padx=16, pady=12)

        tcv, txt = _round_text(f, font=FM, fg=T1, pbg=BK)
        tcv.pack(fill="both", expand=True)
        self._active_textbox = txt          # import button targets this box
        self._register_drop_target(tcv, txt)   # dropped files land here
        PH = "Paste or load an encrypted / signed message…"
        txt.insert("1.0", PH); txt.configure(fg=T3)
        txt.bind("<FocusIn>", lambda _: (txt.get("1.0", "end-1c") == PH) and
                 (txt.delete("1.0", "end"), txt.configure(fg=T1)))

        status = tk.Label(f, text="", font=FS, bg=BK, fg=T3, anchor="w")
        status.pack(fill="x", pady=(8, 0))

        def do_decrypt():
            raw = txt.get("1.0", "end-1c").strip()
            if not raw or raw == PH:
                return
            if "BEGIN PGP" not in raw:
                return messagebox.showerror("Not PGP",
                    "Paste a PGP encrypted or signed message.", parent=self)
            if "BEGIN PGP MESSAGE" in raw and not self._has_key():
                messagebox.showwarning("No Keys",
                    "You have no secret key to decrypt with.", parent=self)
                return self._open_info()
            try:
                text, signer = self._decrypt(raw)   # gpg verifies against the keyring
            except Exception as e:
                return messagebox.showerror("Decrypt Error", str(e), parent=self)
            txt.delete("1.0", "end"); txt.insert("1.0", text); txt.configure(fg=T1)
            if signer:
                vi = self._asset_img("valid.png", 16, tint=GN)
                if vi is not None:
                    status.configure(image=vi, compound="left",
                                     text=f" valid signature from {signer}", fg=GN)
                    status._img = vi                 # keep a ref from GC
                else:
                    status.configure(text=f"✓ valid signature from {signer}", fg=GN)
            else:
                status.configure(image="", text="• processed (no verifiable signature)", fg=T3)
            self._hide_status_later(status)          # clear the line after 4 seconds

        bar = tk.Frame(f, bg=BK)
        bar.pack(fill="x", pady=(10, 0))
        _make_btn(bar, "import file", lambda: self._load_into(txt), w=110, pbg=BK).pack(side="left")
        _make_btn(bar, "decrypt / verify", do_decrypt, w=150, pbg=BK).pack(side="right")

    def _hide_status_later(self, status):
        job = getattr(self, "_status_clear_job", None)
        if job:
            try: self.after_cancel(job)
            except Exception: pass

        def _hide():
            try:
                if status.winfo_exists():
                    status.configure(text="", image="")
            except Exception:
                pass
        self._status_clear_job = self.after(4000, _hide)

    # ── Contact key viewer ────────────────────────────────────────
    def _show_contact_key(self, contact):
        self._clear_main()
        idx = self.active
        self._panel_hdr(
            contact.get("name") or contact.get("keyid", "Key"),
            back_fn=lambda: (self._open(idx) if isinstance(idx, int) else self._welcome())
        )

        f = tk.Frame(self._main, bg=BK)
        f.pack(fill="both", expand=True, padx=24, pady=12)

        for label, val in [("Name", contact.get("name", "")),
                            ("Email", contact.get("email", "")),
                            ("Key ID", contact.get("keyid", "")),
                            ("Created", contact["created"].strftime("%Y-%m-%d")
                             if contact.get("created") else ""),
                            ("Expires", contact["expires"].strftime("%Y-%m-%d")
                             if contact.get("expires") else "never"),
                            ("Fingerprint", contact.get("fpr", ""))]:
            if val:
                _kv_row(f, label, val)

        st = self._key_status(contact)         # expired / expires-soon badge
        if st:
            tk.Label(f, text="⚠ " + st[0], font=FB, bg=BK, fg=st[1]).pack(anchor="w", pady=(4, 0))

        pcv, pt = _round_text(f, fg=T2)
        pcv.pack(fill="both", expand=True, pady=(12, 0))

        try:
            pub = Card.export_pub(contact.get("fpr", "")) or "(no key)"
        except Exception:
            pub = "(no key)"
        pt.insert("1.0", pub)          # left editable → selectable & copyable

        def cp():
            self.clipboard_clear(); self.clipboard_append(pub)
            cpb.configure(text="✓ copied", fg=GN)
            self.after(1800, lambda: cpb.configure(text="copy public key", fg=T2))

        # actions for this key live in its tab
        acts = tk.Frame(f, bg=BK); acts.pack(anchor="w", pady=(10, 0))
        fpr = contact.get("fpr", "")
        sep = lambda: tk.Label(acts, text="  ·  ", font=FS, bg=BK, fg=T4).pack(side="left")

        if contact.get("secret") and fpr != self.my_fpr:
            _link(acts, "set as active key",
                  lambda: (self._set_active(fpr), self._refresh(),
                           self._show_contact_key(contact))).pack(side="left")
            sep()
        cpb = _link(acts, "copy public key", cp); cpb.pack(side="left")
        sep()
        _link(acts, "export public", lambda: self._export_key(fpr, False)).pack(side="left")
        if contact.get("secret") and not contact.get("on_card"):
            sep()
            _link(acts, "export secret", lambda: self._export_key(fpr, True)).pack(side="left")
        sep()
        _link(acts, "delete", lambda: self._delete_from_tab(contact), fg=RD).pack(side="left")

    def _delete_from_tab(self, contact):
        secret = contact.get("secret")
        nm = contact.get("name") or contact.get("keyid", "this key")
        msg = (f"Delete {nm} (including its private key) from the keyring?"
               if secret else f"Remove {nm} from the keyring?")
        if not messagebox.askyesno("Delete key", msg + "\nThis cannot be undone.", parent=self):
            return
        try:
            (Card.delete_secret if secret else Card.delete_pub)(contact["fpr"])
        except Exception as e:
            messagebox.showerror("Error", str(e), parent=self); return
        self.active = None
        self._refresh()
        self._welcome()

    # ── Load a file's contents into a text box (upload button + drag-drop) ──
    def _load_file_into(self, textw, path):
        try:
            with open(path, "r", encoding="utf-8", errors="replace") as fh:
                data = fh.read()
        except Exception as e:
            messagebox.showerror("Read Error", str(e), parent=self); return
        textw.delete("1.0", "end"); textw.insert("1.0", data); textw.configure(fg=T1)

    # ── Native file drag-and-drop onto the message fields ─────────
    #    OLE IDropTarget gives live drag-over events, so we can (a) accept a
    #    drop ONLY over a message field and (b) draw a thin white border on the
    #    field under the cursor while a droppable file hovers it. Falls back to
    #    plain WM_DROPFILES (no hover border) if OLE registration fails.
    def _register_drop_target(self, cv, txt):
        self._drop_targets.append({"cv": cv, "txt": txt})

    def _clear_drop_highlight(self):
        self._drop_hi_want = None
        cur = getattr(self, "_drop_hi", None)
        if cur is not None:
            try:
                if cur.winfo_exists():
                    cur._set_border(DIV)
            except Exception:
                pass
        self._drop_hi = None

    def _drop_hit(self, sx, sy):
        """Hit-test against the CACHED field rects — pure Python, safe to call from
        an OLE callback (no Tk/Tcl reentry during the drag's modal loop)."""
        if sx is None:
            return None
        for (x1, y1, x2, y2, i) in getattr(self, "_drop_rects", ()):
            if x1 <= sx < x2 and y1 <= sy < y2:
                return i
        return None

    def _drop_poll(self):
        """60 ms Tk loop that does ALL the UI work for drag-and-drop: refreshes the
        cached field rects, applies the hover border the callbacks asked for, and
        dispatches a completed drop. The OLE callbacks themselves never touch Tk —
        re-entering Tcl from inside the drag's modal COM dispatch crashed the app."""
        try:
            live, rects = [], []
            for t in self._drop_targets:
                try:
                    if not (t["txt"].winfo_exists() and t["cv"].winfo_exists()):
                        continue
                except Exception:
                    continue
                live.append(t)
                cv = t["cv"]
                try:
                    if cv.winfo_ismapped():
                        x, y = cv.winfo_rootx(), cv.winfo_rooty()
                        rects.append((x, y, x + cv.winfo_width(),
                                      y + cv.winfo_height(), len(live) - 1))
                except Exception:
                    pass
            self._drop_targets = live
            self._drop_rects = rects

            want = getattr(self, "_drop_hi_want", None)
            cv = None
            if want is not None and 0 <= want < len(live):
                cv = live[want]["cv"]
            self._set_drop_hi(cv)

            p = getattr(self, "_drop_pending", None)
            if p is not None:
                self._drop_pending = None
                self._on_drop(*p)
        except Exception:
            pass
        self.after(60, self._drop_poll)

    def _drop_target_at(self, sx, sy):
        """The message field (dict) whose cached rect holds (sx, sy), or None."""
        i = self._drop_hit(sx, sy)
        return self._drop_targets[i] if (i is not None and i < len(self._drop_targets)) else None

    def _set_drop_hi(self, cv):
        """Move the white drop-hover border to `cv` (or clear if None).
        Runs on the Tk mainloop (scheduled via after) — never inside an OLE callback,
        so no PIL/canvas redraw ever happens during the drag's modal message loop."""
        cur = getattr(self, "_drop_hi", None)
        if cv is cur:
            return
        if cur is not None:
            try:
                if cur.winfo_exists(): cur._set_border(DIV)
            except Exception: pass
        self._drop_hi = cv
        if cv is not None:
            try:
                if cv.winfo_exists(): cv._set_border(WT)
            except Exception: pass

    def _on_drop(self, path, sx=None, sy=None):
        self._clear_drop_highlight()
        try:
            if not (path and os.path.isfile(path)):
                _log(f"on_drop: no usable path ({path!r})")
                return
            t = self._drop_target_at(sx, sy)
            _log(f"on_drop: path={os.path.basename(path)!r} at=({sx},{sy}) "
                 f"target={'field' if t else 'none'}")
            if t is not None:                          # only load onto a message field
                self._load_file_into(t["txt"], path)
        except Exception:
            pass

    def _enable_drop(self):
        if not IS_WIN:
            return
        self._drop_hi = None
        try:
            self._enable_drop_ole()
        except Exception:
            try: self._enable_drop_legacy()
            except Exception: pass

    def _enable_drop_ole(self):
        import ctypes
        from ctypes import wintypes
        hwnd = self._hwnd()
        if not hwnd:
            raise RuntimeError("no hwnd")
        ole32 = ctypes.windll.ole32
        ole32.OleInitialize(None)                      # STA; harmless if already inited

        HRESULT, ULONG = ctypes.c_long, ctypes.c_ulong
        DWORD, LPVOID = wintypes.DWORD, ctypes.c_void_p
        PDWORD = ctypes.POINTER(DWORD)
        CF_HDROP, DVASPECT_CONTENT, TYMED_HGLOBAL = 15, 1, 1
        DROPEFFECT_NONE, DROPEFFECT_COPY = 0, 1

        class POINTL(ctypes.Structure):
            _fields_ = [("x", ctypes.c_long), ("y", ctypes.c_long)]

        class FORMATETC(ctypes.Structure):
            _fields_ = [("cfFormat", ctypes.c_ushort), ("ptd", LPVOID),
                        ("dwAspect", DWORD), ("lindex", ctypes.c_long), ("tymed", DWORD)]

        class STGMEDIUM(ctypes.Structure):
            _fields_ = [("tymed", DWORD), ("data", LPVOID), ("pUnkForRelease", LPVOID)]

        def _has_hdrop(pdo):
            try:
                pdo = ctypes.c_void_p(pdo)
                fmt = FORMATETC(CF_HDROP, None, DVASPECT_CONTENT, -1, TYMED_HGLOBAL)
                vt = ctypes.cast(pdo, ctypes.POINTER(ctypes.POINTER(LPVOID)))
                QueryGetData = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID)(vt[0][5])
                return QueryGetData(pdo, ctypes.byref(fmt)) == 0
            except Exception:
                return True

        def _extract_path(pdo):
            try:
                pdo = ctypes.c_void_p(pdo)             # full 64-bit interface pointer
                fmt = FORMATETC(CF_HDROP, None, DVASPECT_CONTENT, -1, TYMED_HGLOBAL)
                med = STGMEDIUM()
                vt = ctypes.cast(pdo, ctypes.POINTER(ctypes.POINTER(LPVOID)))
                GetData = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, LPVOID)(vt[0][3])
                hr = GetData(pdo, ctypes.byref(fmt), ctypes.byref(med))
                if hr != 0:
                    _log(f"drop: GetData(CF_HDROP) failed hr=0x{hr & 0xffffffff:08x}")
                    return None
                shell32 = ctypes.windll.shell32
                shell32.DragQueryFileW.argtypes = [LPVOID, ctypes.c_uint,
                                                   ctypes.c_wchar_p, ctypes.c_uint]
                shell32.DragQueryFileW.restype = ctypes.c_uint
                buf = ctypes.create_unicode_buffer(2048)
                # wrap the HDROP handle so the full 64-bit value is passed (not truncated)
                n = shell32.DragQueryFileW(ctypes.c_void_p(med.data), 0, buf, 2048)
                path = buf.value if n > 0 else None
                ole32.ReleaseStgMedium(ctypes.byref(med))
                return path
            except Exception:
                return None

        def _set_effect(eff, droppable):
            try: eff[0] = DROPEFFECT_COPY if droppable else DROPEFFECT_NONE
            except Exception: pass

        # A POINTL is 8 bytes passed BY VALUE. ctypes struct-by-value into a callback is
        # fragile on x64 and was crashing the app — receive it as one 64-bit int and unpack
        # (low32=x, high32=y; little-endian, so this is correct on both x86 and x64).
        def _unpt(v):
            x, y = v & 0xffffffff, (v >> 32) & 0xffffffff
            if x >= 0x80000000: x -= 0x100000000
            if y >= 0x80000000: y -= 0x100000000
            return x, y

        # ── IDropTarget vtable callbacks (every one is exception-proof) ──
        # QueryInterface MUST refuse interfaces we don't implement. Claiming
        # everything (the old behaviour) made OLE's cross-process marshaling
        # call unrelated vtable slots (IMarshal etc.) on us → hang/crash the
        # moment a REAL Explorer drag first touched the window.
        IID_IUnknown    = b"\x00\x00\x00\x00\x00\x00\x00\x00\xc0\x00\x00\x00\x00\x00\x00\x46"
        IID_IDropTarget = b"\x22\x01\x00\x00\x00\x00\x00\x00\xc0\x00\x00\x00\x00\x00\x00\x46"
        E_NOINTERFACE   = -2147467262                  # 0x80004002

        def qi(this, riid, ppv):
            try:
                iid = ctypes.string_at(riid, 16)
                if iid in (IID_IUnknown, IID_IDropTarget):
                    ppv[0] = this
                    return 0
                ppv[0] = None
                return E_NOINTERFACE
            except Exception:
                return E_NOINTERFACE
        def addref(this):  return 1
        def release(this): return 1
        # Callbacks do ONLY read-only geometry hit-testing; the border repaint is
        # These four run inside the drag's modal COM dispatch. They must NOT touch
        # Tk/Tcl in ANY way — not even after() — because re-entering the Tcl
        # interpreter from this context crashed the process on real Explorer
        # drags. They only do COM work + set plain Python flags; the _drop_poll
        # loop (normal Tk context) applies the border and dispatches the drop.
        def drag_enter(this, pdo, keys, pt, eff):
            try:
                px, py = _unpt(pt)
                self._drag_over_logged = False
                _log(f"dragenter at ({px},{py})")
                self._drag_has = _has_hdrop(pdo)
                _log(f"dragenter: hdrop={self._drag_has}")
                hit = self._drop_hit(px, py) if self._drag_has else None
                _set_effect(eff, hit is not None)
                self._drop_hi_want = hit
                _log("dragenter: done")
            except Exception:
                _set_effect(eff, False)
            return 0
        def drag_over(this, keys, pt, eff):
            try:
                px, py = _unpt(pt)
                if not getattr(self, "_drag_over_logged", False):
                    self._drag_over_logged = True
                    _log(f"dragover first at ({px},{py})")
                hit = self._drop_hit(px, py) if getattr(self, "_drag_has", True) else None
                _set_effect(eff, hit is not None)
                self._drop_hi_want = hit
            except Exception:
                _set_effect(eff, False)
            return 0
        def drag_leave(this):
            self._drop_hi_want = None
            return 0
        def drop(this, pdo, keys, pt, eff):
            try:
                px, py = _unpt(pt)
                _log(f"drop at ({px},{py})")
                path = _extract_path(pdo)
                _log(f"drop: extracted {os.path.basename(path) if path else None!r}")
                _set_effect(eff, path is not None)
                self._drop_hi_want = None
                self._drop_pending = (path, px, py)
            except Exception:
                _set_effect(eff, False)
            return 0

        PT64 = ctypes.c_uint64                          # the by-value POINTL, as one register
        QI = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, ctypes.POINTER(LPVOID))
        AR = ctypes.WINFUNCTYPE(ULONG, LPVOID)
        DE = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, DWORD, PT64, PDWORD)
        DO = ctypes.WINFUNCTYPE(HRESULT, LPVOID, DWORD, PT64, PDWORD)
        DL = ctypes.WINFUNCTYPE(HRESULT, LPVOID)
        DP = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, DWORD, PT64, PDWORD)

        class Vtbl(ctypes.Structure):
            _fields_ = [("QueryInterface", QI), ("AddRef", AR), ("Release", AR),
                        ("DragEnter", DE), ("DragOver", DO), ("DragLeave", DL), ("Drop", DP)]

        class DropTarget(ctypes.Structure):
            _fields_ = [("lpVtbl", ctypes.POINTER(Vtbl))]

        vt = Vtbl(QI(qi), AR(addref), AR(release),
                  DE(drag_enter), DO(drag_over), DL(drag_leave), DP(drop))
        obj = DropTarget(ctypes.pointer(vt))
        ole32.RegisterDragDrop.argtypes = [wintypes.HWND, LPVOID]
        ole32.RegisterDragDrop.restype = HRESULT
        try: ole32.RevokeDragDrop(hwnd)                # in case of a re-init
        except Exception: pass
        hr = ole32.RegisterDragDrop(hwnd, ctypes.byref(obj))
        if hr not in (0, 0x80040101):                  # S_OK / DRAGDROP_E_ALREADYREGISTERED
            raise OSError(f"RegisterDragDrop hr=0x{hr & 0xffffffff:08x}")
        # keep every ctypes object alive for the process lifetime — GC = OLE crash
        self._ole_drop = (qi, addref, release, drag_enter, drag_over, drag_leave, drop, vt, obj)
        if not getattr(self, "_drop_poll_on", False):
            self._drop_poll_on = True
            self.after(80, self._drop_poll)            # UI side of drag-drop lives here
        _log("drag-drop: OLE IDropTarget registered")

    def _enable_drop_legacy(self):
        # Fallback: WM_DROPFILES (drops still land on the field under the cursor,
        # but there's no live hover border).
        import ctypes
        from ctypes import wintypes
        hwnd = self._hwnd()
        if not hwnd:
            return
        user32 = ctypes.windll.user32
        ctypes.windll.shell32.DragAcceptFiles(hwnd, True)
        GWLP_WNDPROC = -4
        LRESULT = ctypes.c_longlong if ctypes.sizeof(ctypes.c_void_p) == 8 else ctypes.c_long
        WPARAM = ctypes.c_ulonglong if ctypes.sizeof(ctypes.c_void_p) == 8 else ctypes.c_ulong
        LPARAM = LRESULT
        user32.CallWindowProcW.restype = LRESULT
        user32.CallWindowProcW.argtypes = [ctypes.c_void_p, wintypes.HWND,
                                           ctypes.c_uint, WPARAM, LPARAM]
        user32.GetWindowLongPtrW.restype = ctypes.c_void_p
        user32.GetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int]
        user32.SetWindowLongPtrW.restype = ctypes.c_void_p
        user32.SetWindowLongPtrW.argtypes = [wintypes.HWND, ctypes.c_int, ctypes.c_void_p]
        WNDPROC = ctypes.WINFUNCTYPE(LRESULT, wintypes.HWND, ctypes.c_uint, WPARAM, LPARAM)
        self._old_wndproc = user32.GetWindowLongPtrW(hwnd, GWLP_WNDPROC)

        def _proc(h, msg, wparam, lparam):
            if msg == 0x0233:                          # WM_DROPFILES
                try: self._handle_drop(wparam)
                except Exception: pass
                try: ctypes.windll.shell32.DragFinish(wparam)
                except Exception: pass
                return 0
            return user32.CallWindowProcW(self._old_wndproc, h, msg, wparam, lparam)

        self._wndproc = WNDPROC(_proc)                 # keep a ref (GC would crash the app)
        user32.SetWindowLongPtrW(hwnd, GWLP_WNDPROC,
                                 ctypes.cast(self._wndproc, ctypes.c_void_p))
        _log("drag-drop: WM_DROPFILES fallback active")

    def _handle_drop(self, hdrop):
        import ctypes
        from ctypes import wintypes
        sx = sy = None
        try:
            pt = wintypes.POINT()
            ctypes.windll.shell32.DragQueryPoint(ctypes.c_void_p(hdrop), ctypes.byref(pt))
            ctypes.windll.user32.ClientToScreen(self._hwnd(), ctypes.byref(pt))
            sx, sy = pt.x, pt.y
        except Exception:
            pass
        buf = ctypes.create_unicode_buffer(2048)
        if ctypes.windll.shell32.DragQueryFileW(ctypes.c_void_p(hdrop), 0, buf, 2048) > 0:
            path = buf.value
            self.after(0, lambda: self._on_drop(path, sx, sy))

    # ── Automated drop self-test (PGPM_SELFTEST_DROP=1) ───────────
    #    Synthesizes a real CF_HDROP data object and invokes our registered
    #    IDropTarget through its raw C vtable pointers — the exact path Windows
    #    uses on a real drop — so the crash surface can be exercised headlessly.
    def _make_file_dataobject(self, path):
        """A minimal in-process IDataObject serving CF_HDROP for `path` — the same
        object shape Explorer hands a drop target. No clipboard involved, so the
        harness can't race other clipboard users (CLIPBRD_E_CANT_OPEN)."""
        import ctypes, struct
        from ctypes import wintypes
        k32 = ctypes.windll.kernel32
        HRESULT, ULONG = ctypes.c_long, ctypes.c_ulong
        DWORD, LPVOID = wintypes.DWORD, ctypes.c_void_p
        CF_HDROP, TYMED_HGLOBAL = 15, 1
        DV_E_FORMATETC, E_NOTIMPL = -2147221404, -2147467263

        class FORMATETC(ctypes.Structure):
            _fields_ = [("cfFormat", ctypes.c_ushort), ("ptd", LPVOID),
                        ("dwAspect", DWORD), ("lindex", ctypes.c_long), ("tymed", DWORD)]

        class STGMEDIUM(ctypes.Structure):
            _fields_ = [("tymed", DWORD), ("data", LPVOID), ("pUnkForRelease", LPVOID)]

        header = struct.pack("<Iiiii", 20, 0, 0, 0, 1)          # DROPFILES: pFiles,pt,fNC,fWide
        blob = header + (path + "\x00\x00").encode("utf-16-le")
        k32.GlobalAlloc.restype = LPVOID
        k32.GlobalLock.restype = LPVOID
        k32.GlobalLock.argtypes = [LPVOID]
        k32.GlobalUnlock.argtypes = [LPVOID]

        def _fresh_hglobal():
            hg = k32.GlobalAlloc(0x0002, len(blob))             # GMEM_MOVEABLE
            p = k32.GlobalLock(hg)
            ctypes.memmove(p, blob, len(blob))
            k32.GlobalUnlock(hg)
            return hg

        def _is_hdrop(pfmt):
            f = ctypes.cast(pfmt, ctypes.POINTER(FORMATETC))[0]
            return f.cfFormat == CF_HDROP and bool(f.tymed & TYMED_HGLOBAL)

        IID_IUnknown    = b"\x00\x00\x00\x00\x00\x00\x00\x00\xc0\x00\x00\x00\x00\x00\x00\x46"
        IID_IDataObject = b"\x0e\x01\x00\x00\x00\x00\x00\x00\xc0\x00\x00\x00\x00\x00\x00\x46"
        E_NOINTERFACE   = -2147467262

        def qi(this, riid, ppv):
            try:
                iid = ctypes.string_at(riid, 16)
                if iid in (IID_IUnknown, IID_IDataObject):
                    ppv[0] = this
                    return 0
                ppv[0] = None
                return E_NOINTERFACE
            except Exception:
                return E_NOINTERFACE
        def addref(this):  return 1
        def release(this): return 1
        def get_data(this, pfmt, pmed):
            try:
                if not _is_hdrop(pfmt):
                    return DV_E_FORMATETC
                med = ctypes.cast(pmed, ctypes.POINTER(STGMEDIUM))
                med[0].tymed = TYMED_HGLOBAL
                med[0].data = _fresh_hglobal()                  # caller frees (ReleaseStgMedium)
                med[0].pUnkForRelease = None
                return 0
            except Exception:
                return DV_E_FORMATETC
        def query_get_data(this, pfmt):
            try: return 0 if _is_hdrop(pfmt) else DV_E_FORMATETC
            except Exception: return DV_E_FORMATETC
        def ni2(this, a):          return E_NOTIMPL
        def ni3(this, a, b):       return E_NOTIMPL
        def ni4(this, a, b, c):    return E_NOTIMPL
        def ni5(this, a, b, c, e): return E_NOTIMPL

        QI  = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, ctypes.POINTER(LPVOID))
        AR  = ctypes.WINFUNCTYPE(ULONG, LPVOID)
        F2  = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID)
        F3  = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, LPVOID)
        F4  = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, LPVOID, LPVOID)
        F5  = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, LPVOID, LPVOID, LPVOID)

        class Vtbl(ctypes.Structure):                            # IDataObject layout
            _fields_ = [("QueryInterface", QI), ("AddRef", AR), ("Release", AR),
                        ("GetData", F3), ("GetDataHere", F3), ("QueryGetData", F2),
                        ("GetCanonicalFormatEtc", F3), ("SetData", F4),
                        ("EnumFormatEtc", F3), ("DAdvise", F5), ("DUnadvise", F2),
                        ("EnumDAdvise", F2)]

        class DataObject(ctypes.Structure):
            _fields_ = [("lpVtbl", ctypes.POINTER(Vtbl))]

        cbs = (QI(qi), AR(addref), AR(release), F3(get_data), F3(ni3), F2(query_get_data),
               F3(ni3), F4(ni4), F3(ni3), F5(ni5), F2(ni2), F2(ni2))
        vt = Vtbl(*cbs)
        obj = DataObject(ctypes.pointer(vt))
        if not hasattr(self, "_selftest_keep"):
            self._selftest_keep = []
        self._selftest_keep.append((cbs, vt, obj, blob))        # GC would tear the vtable down
        return ctypes.c_void_p(ctypes.addressof(obj))

    def _droptest_serve(self):
        """Target mode for the REAL drag-and-drop test (PGPM_DROPTEST_SERVE=1):
        opens Sign/Encrypt, publishes the field's screen rect, writes a heartbeat
        file every 150 ms (stops beating = UI frozen), and exits 0 once a PGP
        message lands in the field via an actual OS drag."""
        d = os.path.join(os.path.expanduser("~"), ".pgp_messenger")
        self.deiconify(); self.geometry("+120+120")
        self._open_encrypt()
        self.update_idletasks(); self.update()

        def write_target():
            try:
                t = self._drop_targets[0]
                cv = t["cv"]
                with open(os.path.join(d, "droptest_target.txt"), "w") as fh:
                    fh.write(f"{cv.winfo_rootx()},{cv.winfo_rooty()},"
                             f"{cv.winfo_width()},{cv.winfo_height()},{os.getpid()}")
                _log("droptest: target rect published")
            except Exception as e:
                _log(f"droptest: publish failed {e!r}")
        self.after(600, write_target)

        hb = os.path.join(d, "droptest_heartbeat.txt")
        def beat():
            try:
                with open(hb, "w") as fh:
                    fh.write(str(datetime.datetime.now().timestamp()))
            except Exception:
                pass
            self.after(150, beat)
        beat()

        def watch():
            try:
                t = self._drop_targets[0] if self._drop_targets else None
                txt = t["txt"].get("1.0", "end-1c") if t else ""
                if "BEGIN PGP MESSAGE" in txt or "SELFTEST" in txt:
                    with open(os.path.join(d, "droptest_content.txt"), "w",
                              encoding="utf-8") as fh:
                        fh.write(txt)
                    _log("droptest: content received — exiting OK")
                    os._exit(0)
            except Exception:
                pass
            self.after(250, watch)
        watch()

    def _selftest_finish(self, ok, detail):
        line = ("PASS " if ok else "FAIL ") + detail
        _log("SELFTEST: " + line)
        try:
            with open(os.path.join(os.path.expanduser("~"), ".pgp_messenger",
                                   "selftest_result.txt"), "w", encoding="utf-8") as fh:
                fh.write(line)
        except Exception:
            pass
        os._exit(0 if ok else 2)

    def _selftest_drop(self, n=40):
        import ctypes, tempfile, time as _t
        try:
            drop = getattr(self, "_ole_drop", None)
            if not drop:
                return self._selftest_finish(False, "no OLE drop target registered")
            obj = drop[-1]
            this = ctypes.addressof(obj)
            vptr = ctypes.cast(this, ctypes.POINTER(ctypes.c_void_p))[0]
            fns = ctypes.cast(vptr, ctypes.POINTER(ctypes.c_void_p * 7))[0]
            HRESULT, DWORD, PT64, LPVOID = (ctypes.c_long, ctypes.c_uint32,
                                            ctypes.c_uint64, ctypes.c_void_p)
            PDWORD = ctypes.POINTER(DWORD)
            SIG = ctypes.WINFUNCTYPE(HRESULT, LPVOID, LPVOID, DWORD, PT64, PDWORD)
            DragEnter, Drop = SIG(fns[3]), SIG(fns[6])           # call through the real C ABI
            self.deiconify(); self.geometry("+120+120"); self.update()
            real = os.environ.get("PGPM_SELFTEST_FILE")           # drop a specific real file?
            real = real if (real and os.path.isfile(real)) else None
            for i in range(n):
                self._open_encrypt()
                # wait for the off-screen panel to swap in and the field to settle on-screen
                tgt = None
                for _ in range(60):
                    self.update_idletasks(); self.update()
                    ts = self._drop_targets
                    if ts and ts[0]["cv"].winfo_ismapped() and ts[0]["cv"].winfo_width() > 5 \
                       and 0 <= ts[0]["cv"].winfo_rootx() < self.winfo_screenwidth():
                        tgt = ts[0]; break
                if tgt is None:
                    return self._selftest_finish(False, f"iter {i}: field never settled on-screen")
                cv, txt = tgt["cv"], tgt["txt"]
                _t.sleep(0.09); self.update()           # let _drop_poll refresh the rect cache
                self.update_idletasks()
                cx = cv.winfo_rootx() + max(6, cv.winfo_width() // 2)
                cy = cv.winfo_rooty() + max(6, cv.winfo_height() // 2)
                pt = ((cy & 0xffffffff) << 32) | (cx & 0xffffffff)
                if real:
                    path = real
                    with open(path, encoding="utf-8", errors="replace") as fh:
                        probe = fh.read()[:40]
                else:
                    probe = f"SELFTEST-{i}-{os.getpid()} the quick brown fox"
                    fd, path = tempfile.mkstemp(suffix=".txt"); os.close(fd)
                    with open(path, "w", encoding="utf-8") as fh: fh.write(probe)
                # the harness feeds the drop through the OS clipboard, which other
                # processes (e.g. clipboard history) occasionally clobber — that's a
                # harness race, not an app bug, so retry the attempt a few times
                ok = False; enter_eff = None; got = ""
                for attempt in range(3):
                    pdo = self._make_file_dataobject(path)
                    if not pdo or not pdo.value:
                        continue
                    eff = DWORD(0); DragEnter(this, pdo, 0, pt, ctypes.byref(eff))
                    enter_eff = eff.value                        # 1 (COPY) = would be accepted
                    if enter_eff != 1:
                        break                                    # a rejection is a REAL failure
                    eff2 = DWORD(0); Drop(this, pdo, 0, pt, ctypes.byref(eff2))
                    for _ in range(30):                          # let the 60 ms poll dispatch it
                        _t.sleep(0.03)
                        self.update(); self.update_idletasks()
                        got = txt.get("1.0", "end-1c")
                        if probe and probe in got: break
                    if probe and probe in got:
                        ok = True; break
                if not real:
                    try: os.remove(path)
                    except Exception: pass
                if enter_eff != 1:
                    return self._selftest_finish(False, f"iter {i}: DragEnter effect={enter_eff} (drop rejected)")
                if not ok:
                    return self._selftest_finish(False, f"iter {i}: field not loaded after 3 attempts, got={got[:60]!r}")
            src = f"real file {real}" if real else "synthesized"
            return self._selftest_finish(True, f"{n} {src} drops OK (load + accept verified)")
        except Exception as e:
            import traceback
            return self._selftest_finish(False, "exception: " + repr(e) + " | " + traceback.format_exc().replace("\n", " ⏎ "))

    # ── Update check (GitHub releases; async, non-blocking) ───────
    @staticmethod
    def _version_newer(a, b):
        def parts(v):
            out = []
            for p in (v or "").split("."):
                digits = "".join(ch for ch in p if ch.isdigit())
                out.append(int(digits) if digits else 0)
            return out
        return parts(a) > parts(b)

    def _check_update(self):
        def worker():
            try:
                import urllib.request
                url = f"https://api.github.com/repos/{GITHUB_REPO}/releases/latest"
                req = urllib.request.Request(url, headers={"User-Agent": "PGPM"})
                with urllib.request.urlopen(req, timeout=6) as resp:
                    data = json.loads(resp.read().decode("utf-8"))
                tag = (data.get("tag_name") or "").lstrip("v")
                if tag and self._version_newer(tag, APP_VERSION):
                    self.after(0, lambda: self._show_update(tag))
            except Exception:
                pass                                   # offline / private repo → silent no-op
        threading.Thread(target=worker, daemon=True).start()

    def _show_update(self, tag):
        bar = getattr(self, "_update_bar", None)
        if bar is None or not bar.winfo_exists():
            return
        import webbrowser
        for w in bar.winfo_children():
            w.destroy()
        url = f"https://github.com/{GITHUB_REPO}/releases/latest"
        lbl = tk.Label(bar, text=f"⬆  PGPM v{tag} is available — click to download",
                       font=FS, bg=BUB_S, fg=WT, cursor="hand2")
        lbl.pack(side="left", pady=4, padx=10)
        lbl.bind("<Button-1>", lambda _: webbrowser.open(url))
        x = tk.Label(bar, text="✕", font=FS, bg=BUB_S, fg=WT, cursor="hand2")
        x.pack(side="right", padx=10)
        x.bind("<Button-1>", lambda _: bar.pack_forget())
        bar.pack(fill="x", before=self._body)

    # ── Sidebar ───────────────────────────────────────────────────
    def _filter_contacts(self):
        if not hasattr(self, "_clist"):
            return
        se = getattr(self, "_search_entry", None)
        txt = self._search_var.get() or ""
        # greyed placeholder = no filter (match the flag AND the literal text, belt-and-suspenders)
        if (se is not None and getattr(se, "_ph", False)) or txt == "Search":
            self._search_filter = ""
        else:
            self._search_filter = txt.strip().lower()
        self._apply_filter()                         # show/hide in place — never rebuild (no flicker)

    def _rebuild_list(self):
        # Called only when the contact SET changes (import/delete/reload). Builds every row
        # once, then applies the active search by showing/hiding — searching itself never
        # destroys/recreates rows, so it (and search-box focus changes) don't flicker.
        for w in self._clist.winfo_children():
            w.destroy()
        self._rows = []
        for i, c in enumerate(self.contacts):
            self._contact_row(i, c)
        self._apply_filter()
        _log(f"rebuild_list: contacts={len(self.contacts)} rows={len(self._rows)} "
             f"filter={self._search_filter!r}")

    def _apply_filter(self):
        """Show/hide existing contact rows for the current search in place (no rebuild)."""
        q = self._search_filter
        for i, r in enumerate(self._rows):
            should = (not q) or (q in r["hay"])
            if should and not r["visible"]:
                # re-pack before the next still-visible row so order is preserved
                nxt = next((self._rows[j]["holder"] for j in range(i + 1, len(self._rows))
                            if self._rows[j]["visible"]), None)
                try:
                    if nxt is not None and nxt.winfo_exists():
                        r["holder"].pack(fill="x", before=nxt)
                    else:
                        r["holder"].pack(fill="x")
                except Exception:
                    r["holder"].pack(fill="x")
                r["visible"] = True
            elif r["visible"] and not should:
                r["holder"].pack_forget()
                r["visible"] = False
        _log(f"apply_filter: q={q!r} visible="
             f"{sum(1 for r in self._rows if r['visible'])}/{len(self._rows)}")

    def _contact_row(self, idx, contact):
        active = (idx == self.active)
        rbg = "#181818" if active else SB

        holder = tk.Frame(self._clist, bg=SBG)     # per-contact container (shown/hidden on search)
        holder.pack(fill="x")
        row = tk.Frame(holder, bg=rbg, cursor="hand2")
        row.pack(fill="x")

        mid = tk.Frame(row, bg=rbg)
        mid.pack(side="left", fill="x", expand=True, padx=14, pady=10)

        nm = contact.get("name") or contact.get("email") or contact.get("keyid", "")[:16]
        if contact.get("secret"):
            nm += "  (you)"
        nl = tk.Label(mid, text=nm, font=FB,
                      bg=rbg, fg=T1, anchor="w")
        nl.pack(anchor="w")

        sub = contact.get("email") or (
            (contact.get("keyid", "")[:16] + "…") if contact.get("keyid") else "")
        sl = None
        if sub:
            sl = tk.Label(mid, text=sub, font=FS, bg=rbg, fg=T3, anchor="w")
            sl.pack(anchor="w")

        badge = None
        st = self._key_status(contact)         # expired / expires-soon badge
        if st:
            badge = tk.Label(mid, text="⚠ " + st[0], font=(UI, 8), bg=rbg, fg=st[1], anchor="w")
            badge.pack(anchor="w")

        tb = _icon_widget(row, 19, "trash", lambda i=idx: self._remove(i),
                          fg=T4, hover=RD, pbg=rbg, tip="Delete key")
        tb.pack(side="right", padx=(0, 12))

        tk.Frame(holder, bg=DIV, height=1).pack(fill="x")

        widgets = [row, mid, nl, tb] + ([sl] if sl else []) + ([badge] if badge else [])
        hay = " ".join([contact.get("name", ""), contact.get("email", ""),
                        contact.get("keyid", ""), contact.get("fpr", "")]).lower()
        fpr = contact.get("fpr")
        self._rows.append({"idx": idx, "holder": holder, "row": row, "fpr": fpr,
                           "hay": hay, "visible": True, "widgets": widgets})

        def enter(_):
            if self.active != idx and getattr(self, "_drag", None) is None:
                for w in widgets:
                    try: w.configure(bg=HOV)
                    except Exception: pass

        def leave(_):
            if self.active != idx and getattr(self, "_drag", None) is None:
                for w in widgets:
                    try: w.configure(bg=SB)
                    except Exception: pass

        # click to open · press-and-drag to reorder (a small threshold tells them apart)
        for w in [row, mid, nl]:
            w.bind("<ButtonPress-1>",   lambda e, f=fpr: self._row_press(e, f))
            w.bind("<B1-Motion>",       lambda e, f=fpr: self._row_motion(e, f))
            w.bind("<ButtonRelease-1>", lambda e, f=fpr: self._row_release(e, f))
            w.bind("<Enter>", enter)
            w.bind("<Leave>", leave)

    # ── Drag-to-reorder contacts (press-drag a row up/down to arrange) ──
    def _contact_index_by_fpr(self, fpr):
        return next((i for i, c in enumerate(self.contacts)
                     if c.get("fpr") == fpr), None)

    def _row_by_fpr(self, fpr):
        return next((r for r in getattr(self, "_rows", []) if r.get("fpr") == fpr), None)

    def _row_press(self, e, fpr):
        r = self._row_by_fpr(fpr)
        self._drag = {"fpr": fpr, "y0": e.y_root,
                      "holder": r["holder"] if r else None, "moved": False}

    def _drag_rows(self, fpr):
        """Visible rows except the dragged one, top-to-bottom."""
        rows = [r for r in self._rows
                if r.get("fpr") != fpr and r["holder"].winfo_ismapped()]
        rows.sort(key=lambda r: r["holder"].winfo_rooty())
        return rows

    def _row_motion(self, e, fpr):
        d = getattr(self, "_drag", None)
        if not d or d.get("fpr") != fpr or d["holder"] is None:
            return
        h = d["holder"]
        sf = self._clist_sf                          # list viewport — ghost is clipped inside it
        if not d["moved"]:
            if abs(e.y_root - d["y0"]) < 6:          # tiny wobble is still a click
                return
            d["moved"] = True
            d["off"] = e.y_root - h.winfo_rooty()               # grab point inside the row
            d["gx"]  = h.winfo_rootx() - sf.winfo_rootx()
            d["gw"]  = h.winfo_width()
            d["gh"]  = max(1, h.winfo_height())
            # where the row currently sits among the others (the gap starts here)
            top = h.winfo_rooty()
            rows = self._drag_rows(fpr)
            d["gap_idx"] = sum(1 for r in rows if r["holder"].winfo_rooty() < top)
            gx0 = sf.winfo_rootx() + d["gx"]
            gy0 = max(sf.winfo_rooty(), h.winfo_rooty())
            d["ghost"] = self._make_drag_ghost(h, fpr, d["gh"], gx0, gy0, d["gw"])
            try: h.pack_forget()                                # pull the row OUT of the flow
            except Exception: pass
            d["gap"] = self._insert_gap(rows, d["gap_idx"], d["gh"], animate=False)
        g = d.get("ghost")
        if g is not None and g.winfo_exists():
            # clamp inside the list viewport so it never covers the search bar / toolbar
            top = sf.winfo_rooty()
            sx = sf.winfo_rootx() + d["gx"]
            sy = e.y_root - d["off"]
            sy = max(top, min(sy, top + max(0, sf.winfo_height() - d["gh"])))
            try:
                g.geometry(f"{d['gw']}x{d['gh']}+{sx}+{sy}")
            except Exception:
                pass
        self._update_gap(d, e.y_root)

    def _row_release(self, e, fpr):
        d = getattr(self, "_drag", None)
        self._drag = None
        if not d:
            return
        for k in ("ghost", "gap"):
            w = d.get(k)
            if w is not None:
                try: w.destroy()
                except Exception: pass
        if not d["moved"]:                           # a plain click → open the chat
            idx = self._contact_index_by_fpr(fpr)
            if idx is not None:
                self._show(("chat", idx), lambda i=idx: self._open(i))
            return
        self._commit_reorder(fpr, d.get("gap_idx"))
        self.after(30, self._force_repaint)          # wipe any leftover drag trails

    def _make_drag_ghost(self, holder, fpr, height, x, y, width):
        """A floating copy of the row that tracks the cursor while dragging.
        It is a tiny borderless toplevel — its own OS-composited surface — so
        moving it fast can NEVER smear paint trails into the app window (a
        place()d child frame did exactly that). It is created already at its
        position and mapped immediately: withdraw()+deiconify() does not
        reliably re-map an overrideredirect toplevel on Windows (the ghost —
        and with it the whole dragged contact — went invisible)."""
        c = next((c for c in self.contacts if c.get("fpr") == fpr), None)
        if c is None:
            return None
        name = c.get("name") or c.get("email") or (c.get("keyid", "")[:16]) or "key"
        sub = c.get("email") or ((c.get("keyid", "")[:16] + "…") if c.get("keyid") else "")
        LB = "#242424"
        g = tk.Toplevel(self)
        g.overrideredirect(True)
        g.geometry(f"{width}x{height}+{x}+{y}")      # in place BEFORE it first maps
        try:
            g.attributes("-topmost", True)
        except Exception:
            pass
        g.configure(bg=WT)                           # 1px white outline via padding
        box = tk.Frame(g, bg=LB)
        box.pack(fill="both", expand=True, padx=1, pady=1)
        inner = tk.Frame(box, bg=LB)
        inner.pack(fill="both", expand=True, padx=13, pady=9)
        tk.Label(inner, text=name, font=FB, bg=LB, fg=T1, anchor="w").pack(anchor="w")
        if sub:
            tk.Label(inner, text=sub, font=FS, bg=LB, fg=T3, anchor="w").pack(anchor="w")
        try:
            g.update_idletasks()                     # map it now, at the right spot
            # alpha must be set AFTER the window maps: pre-map it never becomes
            # visible at all (verified by on-screen pixel test on this machine)
            g.attributes("-alpha", 0.93)
        except Exception:
            pass
        return g

    # ── The make-room gap: an empty slot that smoothly opens where the row would drop ──
    def _insert_gap(self, rows, idx, H, animate=True):
        gap = tk.Frame(self._clist, bg=SBG, height=(1 if animate else H))
        try:
            if idx < len(rows):
                gap.pack(fill="x", before=rows[idx]["holder"])
            else:
                gap.pack(fill="x")
        except Exception:
            gap.pack(fill="x")
        if animate:
            self._grow_gap(gap, H)
        return gap

    def _grow_gap(self, gap, H, step=0):
        steps = 5
        if not gap.winfo_exists():
            return
        gap.configure(height=max(1, int(H * (step + 1) / steps)))
        if step + 1 < steps:
            self.after(15, lambda: self._grow_gap(gap, H, step + 1))

    def _collapse_gap(self, gap, H, step=0):
        steps = 5
        if not gap.winfo_exists():
            return
        h = int(H * (steps - step - 1) / steps)
        if h <= 1 or step + 1 >= steps:
            try: gap.destroy()
            except Exception: pass
            return
        gap.configure(height=h)
        self.after(15, lambda: self._collapse_gap(gap, H, step + 1))

    def _update_gap(self, d, y_root):
        """While dragging: if the cursor crossed into a new slot, glide the gap there —
        the neighbouring rows smoothly move down/up to show where the drop will land."""
        gap = d.get("gap")
        if gap is not None and gap.winfo_exists() and gap.winfo_height() < d["gh"] - 1:
            return                                   # still gliding — settle before re-deciding
        rows = self._drag_rows(d["fpr"])
        idx = len(rows)
        for i, r in enumerate(rows):
            h = r["holder"]
            try: mid = h.winfo_rooty() + h.winfo_height() / 2
            except Exception: continue
            if y_root < mid:
                idx = i; break
        if idx == d.get("gap_idx"):
            return
        d["gap_idx"] = idx
        old = d.get("gap")
        if old is not None:
            self._collapse_gap(old, d["gh"])         # old slot closes…
        d["gap"] = self._insert_gap(rows, idx, d["gh"])  # …new slot opens

    def _commit_reorder(self, fpr, idx):
        """Drop the dragged contact at slot `idx` (where the gap is); persist; rebuild once."""
        active_fpr = None
        if isinstance(self.active, int) and 0 <= self.active < len(self.contacts):
            active_fpr = self.contacts[self.active].get("fpr")
        rows = self._drag_rows(fpr)
        order = [r["fpr"] for r in rows]
        if idx is None or not (0 <= idx <= len(order)):
            idx = len(order)
        order.insert(idx, fpr)
        by_fpr = {c.get("fpr"): c for c in self.contacts}
        new_contacts = [by_fpr[f] for f in order if f in by_fpr]
        seen = set(order)
        for c in self.contacts:                       # keep any search-hidden contacts
            if c.get("fpr") not in seen:
                new_contacts.append(c)
        self.contacts = new_contacts
        self._cfg_set("contact_order", [c.get("fpr") for c in self.contacts])
        if active_fpr is not None:
            self.active = self._contact_index_by_fpr(active_fpr)
        self._rebuild_list()

    def _highlight_active(self, new_idx):
        """Move the selection highlight in place — no list rebuild, no flicker."""
        self.active = new_idx
        for r in getattr(self, "_rows", []):
            bg = "#181818" if r["idx"] == new_idx else SB
            for w in r["widgets"]:
                try: w.configure(bg=bg)
                except Exception: pass

    # ── Contact panel (encrypt / decrypt zone) ────────────────────
    def _open(self, idx):
        self._highlight_active(idx)     # in-place; sidebar is not rebuilt
        self._build_chat(idx)
        self._view = ("chat", idx)

    def _build_chat(self, idx):
        self._clear_main()
        c = self.contacts[idx]

        # Header: contact name + key + trash icons
        h = tk.Frame(self._main, bg=HDR, height=52)
        h.pack(fill="x")
        h.pack_propagate(False)

        _icon_widget(h, 16, "back", self._welcome, fg=T3, hover=T1).pack(
            side="left", padx=(14, 0), pady=16)
        nm = c.get("name") or c.get("email") or c.get("keyid", "Key")
        tk.Label(h, text=nm, font=FB, bg=HDR, fg=T1, padx=8).pack(side="left", pady=10)

        # 3-dot menu → view this key's info (selectable / copyable)
        vk = _icon_widget(h, 20, "more", lambda: self._show_contact_key(c),
                          fg=T3, hover=T1, tip="Key info")
        vk.pack(side="right", padx=(4, 14), pady=16)

        # Typing zone under the name (rounded, fills the panel)
        zone = tk.Frame(self._main, bg=BK)
        zone.pack(fill="both", expand=True, padx=14, pady=(12, 8))
        zcv, self._msg_in = _round_text(zone, font=FM, fg=T1, pbg=BK)
        zcv.pack(fill="both", expand=True)
        self._register_drop_target(zcv, self._msg_in)   # dropped files land here

        PHZ = "Type a message to encrypt…"
        self._msg_in.insert("1.0", PHZ)
        self._msg_in.configure(fg=T3)

        def _zfi(_):
            if self._msg_in.get("1.0", "end-1c") == PHZ:
                self._msg_in.delete("1.0", "end")
                self._msg_in.configure(fg=T1)

        def _zfo(_):
            if not self._msg_in.get("1.0", "end-1c").strip():
                self._msg_in.insert("1.0", PHZ)
                self._msg_in.configure(fg=T3)

        self._msg_in.bind("<FocusIn>", _zfi)
        self._msg_in.bind("<FocusOut>", _zfo)

        # Signing / verification status (updated on encrypt / decrypt)
        self._chat_status = tk.Label(self._main, text="", font=FS, bg=BK, fg=T3, anchor="w")
        self._chat_status.pack(fill="x", padx=16, pady=(0, 2))

        # Buttons: Encrypt · Decrypt · shuffle · Copy
        # (encryption always also targets your own key so you can re-open sent messages)
        br = tk.Frame(self._main, bg=BK)
        br.pack(fill="x", padx=14, pady=(0, 14))
        _make_btn(br, "Encrypt", lambda: self._encrypt_zone(idx), w=130).pack(side="left")
        _make_btn(br, "Decrypt", self._decrypt_zone, w=130).pack(side="left", padx=(8, 0))
        _make_btn(br, "Copy",    self._copy_zone,    w=90, bold=True).pack(side="right")

    # ── Encrypt / Decrypt in the typing zone ──────────────────────
    _PHZ = "Type a message to encrypt…"

    def _zone_text(self):
        t = self._msg_in.get("1.0", "end-1c")
        return "" if t.strip() == self._PHZ else t.strip()

    def _zone_set(self, text):
        self._msg_in.delete("1.0", "end")
        self._msg_in.insert("1.0", text)
        self._msg_in.configure(fg=T1)

    def _set_chat_status(self, text, color=T3):
        lbl = getattr(self, "_chat_status", None)
        if lbl is not None:
            try: lbl.configure(text=text, fg=color)
            except Exception: pass

    def _encrypt_zone(self, idx):
        text = self._zone_text()
        if not text:
            return
        fpr = self.contacts[idx].get("fpr")
        if not fpr:
            messagebox.showerror("No Key", "This entry has no key.", parent=self); return
        recips = [fpr]
        if self.my_fpr and self.my_fpr not in recips:
            recips.append(self.my_fpr)               # also encrypt to me (keep a readable copy)
        try:
            enc = Card.encrypt(text, recips)         # no signing
        except Exception as e:
            messagebox.showerror("Encryption Error", str(e), parent=self); return
        self._zone_set(enc)
        self._set_chat_status("")

    def _decrypt_zone(self):
        if not self._has_key():
            messagebox.showwarning("No Keys", "You have no secret key to decrypt with.",
                                   parent=self); return
        raw = self._zone_text()
        if not raw:
            return
        if "-----BEGIN PGP MESSAGE-----" not in raw:
            messagebox.showwarning("Not PGP",
                "Paste a PGP encrypted message block to decrypt.", parent=self); return
        try:
            plain, signer = self._decrypt(raw)
        except Exception as e:
            messagebox.showerror("Decryption Error", str(e), parent=self); return
        self._zone_set(plain)
        self._set_chat_status("")            # messages aren't signed anymore

    def _copy_zone(self):
        text = self._zone_text()
        if text:
            self.clipboard_clear(); self.clipboard_append(text)

    def _rewrite_zone(self):
        """Rewrite the compose box for anonymity (offline) without blocking the UI."""
        text = self._zone_text()
        if not text:
            return
        res = [None]

        def worker():
            try: res[0] = _anonymize_text(text)
            except Exception: res[0] = text

        t = threading.Thread(target=worker, daemon=True); t.start()

        def poll():
            if t.is_alive():
                self.after(80, poll); return
            if res[0] is not None:
                self._zone_set(res[0])
        self.after(80, poll)

    # ── Contact management ────────────────────────────────────────
    def _remove(self, idx):
        c = self.contacts[idx]
        nm = c.get("name") or c.get("keyid", "this key")
        if c.get("secret"):
            if not messagebox.askyesno("Delete secret key",
                f"{nm} is one of YOUR keys (has a private key).\n\n"
                "Delete it from the GnuPG keyring? This cannot be undone.", parent=self):
                return
            try:
                Card.delete_secret(c["fpr"])
            except Exception as e:
                messagebox.showerror("Error", str(e), parent=self); return
        else:
            if not messagebox.askyesno("Remove key",
                f"Remove {nm} from the keyring?", parent=self):
                return
            try:
                Card.delete_pub(c["fpr"])
            except Exception as e:
                messagebox.showerror("Error", str(e), parent=self); return
        self.active = None
        self._refresh()
        self._welcome()


if __name__ == "__main__":
    if not _acquire_single_instance():
        try:
            from tkinter import messagebox
            _r = tk.Tk(); _r.withdraw()
            messagebox.showinfo("PGPM", "PGPM is already running.")
            _r.destroy()
        except Exception:
            pass
        sys.exit(0)
    App().mainloop()
