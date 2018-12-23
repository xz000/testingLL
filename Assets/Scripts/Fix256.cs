using System;
using UnityEngine;

public struct Fix256
{
    private static int UnderInt = 256;
    private long UpLong;

    public float value
    {
        get
        {
            return (float)UpLong / UnderInt;
        }
    }

    public Fix256(float t)
    {
        UpLong = (long)(t * UnderInt);
    }
    public Fix256(int t)
    {
        UpLong = (long)t * UnderInt;
    }
    public Fix256(double t)
    {
        UpLong = (long)(t * UnderInt);
    }
    public Fix256(int t1, int t2)
    {
        UpLong = ((long)t1 * UnderInt) / t2;
    }

    public static Fix256 zero = new Fix256(0);

    public static Fix256 operator +(Fix256 t1, Fix256 t2)
    {
        Fix256 temp;
        temp.UpLong = t1.UpLong + t2.UpLong;
        return temp;
    }
    public static Fix256 operator -(Fix256 t1, Fix256 t2)
    {
        Fix256 temp;
        temp.UpLong = t1.UpLong - t2.UpLong;
        return temp;
    }
    public static Fix256 operator -(Fix256 t)
    {
        t.UpLong = -t.UpLong;
        return t;
    }
    public static Fix256 operator *(Fix256 t1, Fix256 t2)
    {
        Fix256 temp;
        temp.UpLong = (t1.UpLong * t2.UpLong) / UnderInt;
        return temp;
    }
    public static Fix256 operator /(Fix256 t1, Fix256 t2)
    {
        Fix256 temp;
        temp.UpLong = (t1.UpLong * UnderInt) / t2.UpLong;
        return temp;
    }
    public static Fix256 operator /(Fix256 t1, int t2)
    {
        Fix256 temp;
        temp.UpLong = t1.UpLong / t2;
        return temp;
    }
    public static bool operator ==(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong == t2.UpLong;
    }
    public static bool operator !=(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong != t2.UpLong;
    }
    public static bool operator >(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong > t2.UpLong;
    }
    public static bool operator <(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong < t2.UpLong;
    }
    public static bool operator >=(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong >= t2.UpLong;
    }
    public static bool operator <=(Fix256 t1, Fix256 t2)
    {
        return t1.UpLong <= t2.UpLong;
    }

    public static Fix256 Sqrt(Fix256 t)
    {
        double v = (double)t.UpLong / UnderInt;
        double d = Math.Sqrt(v);
        Fix256 f = new Fix256(d);
        return f;
    }

    public override bool Equals(object obj)
    {
        if (obj is Fix256)
        {
            Fix256 f = (Fix256)obj;
            return f.UpLong == UpLong;
        }
        return false;
    }
    public override string ToString()
    {
        return value.ToString();
    }
    public override int GetHashCode()
    {
        return 0;
    }

    public static float F256f(float t)
    {
        Fix256 f = new Fix256(t);
        return f.value;
    }

    public static Vector2 F256v2(Vector2 v2)
    {
        Vector2 fv2 = new Vector2(F256f(v2.x), F256f(v2.y));
        return fv2;
    }
}
