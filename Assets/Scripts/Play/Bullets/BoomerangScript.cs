using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class BoomerangScript : MonoBehaviour
{
    public float a = 0.2f;
    public Rigidbody2D senderRB;
    public Rigidbody2D selfRB;
	
	void FixedUpdate() {
        selfRB.velocity += (senderRB.position - selfRB.position) * a * Time.fixedDeltaTime;
	}

    void OnCollisionEnter2D(Collision2D collision)
    {
        Fix64Vector2 sp = -(Fix64Vector2)selfRB.velocity;
        Fix64Vector2 vp = (Fix64Vector2)(Vector2)(collision.transform.position - transform.position);
        selfRB.velocity = Fix64Vector2.MirrorBy(sp, vp).ToV2();
    }
}
