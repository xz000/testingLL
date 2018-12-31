using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class RockExplode : MonoBehaviour
{
    public float damage = 10;
    public float bombforce = 18;
    public float pushtime = 1;
    private float timetosing = 2;
    private float timesinged = 0;

    // Use this for initialization
    void Start ()
    {
        timesinged = 0;
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
        //photonView.RPC("RealSkill", PhotonTargets.All);
        GetComponent<DestroyScript>().Destroyself();
    }

    public void RealSkill()
    {
        //gameObject.GetComponent<MoveScript>().stopwalking();
        float radius = transform.lossyScale.x / 2;
        Vector2 actionplacev = transform.position;
        Fix64Vector2 actionplacef = (Fix64Vector2)actionplacev;
        Collider2D[] colliders = Physics2D.OverlapCircleAll(actionplacev, radius);
        foreach (Collider2D hit in colliders)
        {
            HPScript hp = hit.GetComponent<HPScript>();
            if (hp != null)
            {
                hp.GetHurt(damage);
                Rigidbody2D rb = hit.GetComponent<Rigidbody2D>();
                if (rb != null)
                {
                    Fix64Vector2 explforce;
                    explforce = (Fix64Vector2)rb.position - actionplacef;
                    hit.GetComponent<RBScript>().GetPushed(explforce.normalized() * (Fix64)bombforce, pushtime);
                }
            }
        }
    }

    private void OnDestroy()
    {
        RealSkill();
    }
}
