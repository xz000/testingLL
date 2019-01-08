using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class RockExplode : MonoBehaviour
{
    public Fix64 damage = (Fix64)10;
    public float bombforce = 8;
    public float pushtime = 1;
    private float timetosing = 2;
    private float timesinged = 0;
    float radius;
    //Fix64 rf;
    //Fix64 rfs;

    // Use this for initialization
    void Start ()
    {
        timesinged = 0;
        radius = transform.lossyScale.x / 2;
        //rf = (Fix64)radius + Fix64.One / (Fix64)2;
        //rfs = rf * rf;
    }
	
	// Update is called once per frame
	void FixedUpdate()
    {
        timesinged += Time.fixedDeltaTime;
        if (timesinged >= timetosing)
            Skill();
    }

    public void Skill()
    {
        GetComponent<DestroyScript>().Destroyself();
    }

    public void RealSkill()
    {
        Fix64 ff = (Fix64)bombforce;
        Vector2 actionplacev = transform.position;
        Fix64Vector2 actionplacef = (Fix64Vector2)actionplacev;
        Collider2D[] colliders = Physics2D.OverlapCircleAll(actionplacev, radius + 0.1f);
        foreach (Collider2D hit in colliders)
        {
            /*Fix64 dfs = ((Fix64Vector2)(Vector2)hit.transform.position - actionplacef).LengthSquare();
            if (dfs > rfs)
                continue;*/
            HPScript hp = hit.GetComponent<HPScript>();
            if (hp != null)
            {
                hp.GetHurt(damage);
                Rigidbody2D rb = hit.GetComponent<Rigidbody2D>();
                if (rb != null)
                {
                    Fix64Vector2 explforce;
                    explforce = (Fix64Vector2)rb.position - actionplacef;
                    hit.GetComponent<RBScript>().GetPushed(explforce.normalized() * ff, pushtime);
                }
            }
        }
    }

    private void OnDestroy()
    {
        RealSkill();
    }
}
