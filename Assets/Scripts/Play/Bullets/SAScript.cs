using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SAScript : MonoBehaviour
{

    private float pasttime;
    public float maxtime = 3;
    public float BulletSpeed = 6;
    public GameObject fireball;
    public GameObject sender;
    public Rigidbody2D selfRB2D;
    float firetime = 0.2f;
    Fix64Vector2 drt;
    bool firestart = false;

    void FixedUpdate()
    {
        pasttime += Time.fixedDeltaTime;
        if (pasttime >= maxtime)
        {
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
        if (firestart && firetime >= 0.2f)
            FFF();
        firetime += Time.fixedDeltaTime;
    }

    public void StartFire()
    {
        drt = ((Fix64Vector2)selfRB2D.velocity).normalized();
        firestart = true;
    }

    public void FFF()
    {
        DoFire((drt * (Fix64)BulletSpeed).ToV2());
        drt = drt.CCWTurn((Fix64)1);
        firetime -= 0.2f;
    }

    void DoFire(Vector2 speed2d)
    {
        fireball.GetComponent<SABulletScript>().sender = sender;
        GameObject bullet = Instantiate(fireball, selfRB2D.position, Quaternion.identity);
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
    }
}
