using System;
using System.IO;
using System.Runtime.CompilerServices;
using UnityEngine;

namespace FixMath
{
    public struct Fix64Vector2
    {
        public Fix64 x;
        public Fix64 y;
        public static readonly Fix64Vector2 Zero = new Fix64Vector2();

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

        public static Fix64 DotMulti(Fix64Vector2 fv2a, Fix64Vector2 fv2b)
        {
            return fv2a.x * fv2b.x + fv2a.y * fv2b.y;
        }

        public void CCWTurn(Fix64 angle)
        {
            Fix64 c = Fix64.Cos(angle);
            Fix64 s = Fix64.Sin(angle);
            Fix64 ox = x;
            x = x * c + y * s;
            y = y * c - ox * s;
        }
    }
}
