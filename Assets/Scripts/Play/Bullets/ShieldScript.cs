using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class ShieldScript : MonoBehaviour
{
    public GameObject sender;
    public float maxtime = 2;
    float timepsd = 0;

    void FixedUpdate()
    {
        timepsd += Time.fixedDeltaTime;
        transform.position = sender.transform.position;
        if (timepsd >= maxtime || sender == null)
            Destroy(gameObject);
    }

    void OnTriggerEnter2D(Collider2D collision)
    {
        if ((collision.transform.position - transform.position).sqrMagnitude < 0.99)
            return;
        if (collision.GetComponent<MoveScript>() != null)
        {
            Fix64Vector2 sp = collision.GetComponent<MoveScript>().Givenvelocity;
            Fix64Vector2 vp = (Fix64Vector2)(Vector2)transform.position - (Fix64Vector2)(Vector2)collision.transform.position;
            collision.GetComponent<MoveScript>().Givenvelocity = Fix64Vector2.MirrorBy(sp, vp);
            return;
        }
        if (collision.GetComponent<Rigidbody2D>() != null)
        {
            Fix64Vector2 sp = -(Fix64Vector2)collision.GetComponent<Rigidbody2D>().velocity;
            Fix64Vector2 vp = (Fix64Vector2)(Vector2)transform.position - (Fix64Vector2)(Vector2)collision.transform.position;
            collision.GetComponent<Rigidbody2D>().velocity = Fix64Vector2.MirrorBy(sp, vp).ToV2();
        }
    }
}
