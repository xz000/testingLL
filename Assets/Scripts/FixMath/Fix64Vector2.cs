using System;
using System.IO;
using System.Runtime.CompilerServices;
using UnityEngine;

namespace FixMath
{
    public struct Fix64Vector2
    {
        readonly Fix64 x;
        readonly Fix64 y;
        public static readonly Fix64Vector2 Zero = new Fix64Vector2();
        public static readonly Fix64Vector2 Up = new Fix64Vector2(Fix64.Zero, Fix64.One);
        public static readonly Fix64Vector2 Down = new Fix64Vector2(Fix64.Zero, -Fix64.One);
        public static readonly Fix64Vector2 Left = new Fix64Vector2(-Fix64.One, Fix64.Zero);
        public static readonly Fix64Vector2 Right = new Fix64Vector2(Fix64.One, Fix64.Zero);

        public Fix64Vector2(Fix64 xv,Fix64 yv)
        {
            x = xv;
            y = yv;
        }

        public Fix64Vector2(Vector2 v2)
        {
            x = (Fix64)v2.x;
            y = (Fix64)v2.y;
        }

        public static explicit operator Fix64Vector2(Vector2 v2)
        {
            Fix64Vector2 Fv2 = new Fix64Vector2(v2);
            return Fv2;
        }

        public Vector2 ToV2()
        {
            Vector2 v2 = new Vector2((float)x, (float)y);
            return v2;
        }

        public Fix64 LengthSquare()
        {
            return x * x + y * y;
        }

        public Fix64 Length()
        {
            return Fix64.Sqrt(LengthSquare());
        }

        public Fix64Vector2 normalized()
        {
            Fix64 l = Length();
            return new Fix64Vector2(x / l, y / l);
        }

        public static Fix64 DotMulti(Fix64Vector2 fv2a, Fix64Vector2 fv2b)
        {
            return fv2a.x * fv2b.x + fv2a.y * fv2b.y;
        }

        public static Fix64Vector2 MirrorBy(Fix64Vector2 origin,Fix64Vector2 mirror)
        {
            Fix64Vector2 mn = mirror.normalized();
            Fix64 pl = DotMulti(origin, mn);
            Fix64Vector2 m2p = mn * pl * (Fix64)2;
            return m2p - origin;
        }

        public static explicit operator Vector2(Fix64Vector2 v)
        {
            return v.ToV2();
        }

        public Fix64Vector2 CCWTurn(Fix64 angle)
        {
            Fix64 c = Fix64.Cos(angle);
            Fix64 s = Fix64.Sin(angle);
            return new Fix64Vector2(x * c + y * s, y * c - x * s);
        }

        public static Fix64Vector2 operator +(Fix64Vector2 a, Fix64Vector2 b)
        {
            return new Fix64Vector2(a.x + b.x, a.y + b.y);
        }
        public static Fix64Vector2 operator -(Fix64Vector2 a, Fix64Vector2 b)
        {
            return new Fix64Vector2(a.x - b.x, a.y - b.y);
        }
        public static Fix64Vector2 operator *(Fix64Vector2 a, Fix64 b)
        {
            return new Fix64Vector2(a.x * b, a.y * b);
        }
        public static Fix64Vector2 operator *(Fix64 b, Fix64Vector2 a)
        {
            return new Fix64Vector2(a.x * b, a.y * b);
        }
        public static Fix64Vector2 operator /(Fix64Vector2 a, Fix64 b)
        {
            return new Fix64Vector2(a.x / b, a.y / b);
        }
        public static Fix64Vector2 operator -(Fix64Vector2 a)
        {
            return new Fix64Vector2(-a.x, -a.y);
        }
        public static bool operator ==(Fix64Vector2 t1, Fix64Vector2 t2)
        {
            return t1.x == t2.x && t1.y == t2.y;
        }
        public static bool operator !=(Fix64Vector2 t1, Fix64Vector2 t2)
        {
            return t1.x != t2.x || t1.y != t2.y;
        }
        public override bool Equals(object obj)
        {
            if (obj is Fix64Vector2)
            {
                Fix64Vector2 f = (Fix64Vector2)obj;
                return x == f.x && y == f.y;
            }
            return false;
        }
        public override int GetHashCode()
        {
            return ToV2().GetHashCode();
        }
    }
}
