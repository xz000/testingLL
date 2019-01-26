using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class ReturnScript : MonoBehaviour
{
    public Rigidbody2D Target;
    public Rigidbody2D selfrb2d;
    public int Speed;

    void FixedUpdate()
    {
        if (Target == null)
            return;
        Fix64Vector2 direction = (Fix64Vector2)Target.position - (Fix64Vector2)selfrb2d.position;
        if (direction.LengthSquare() <= (Fix64)0.3)
            gethome();
        selfrb2d.velocity = (direction.normalized() * (Fix64)Speed).ToV2();
    }

    void gethome()
    {
        Target.gameObject.GetComponent<SkillT3b>().ResetCD();
        GetComponent<DestroyScript>().Destroyself();
    }
}
