using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class JumpBulletScript : MonoBehaviour
{

    public GameObject sender;
    GameObject Target;
    public float speed = 20;
    public Rigidbody2D bulletRB2D;
    Rigidbody2D targetRB2D;
    public Fix64 Damage;
    private Fix64 damageRatio = Fix64.One;
    private Fix64 damageMinus = Fix64.One / (Fix64)5;
    //public int maxjumptime;
    float currenttime = 0;
    public float maxtime = 1;
    //bool beginning = true;

	// Use this for initialization
	void Start () {

	}
	
	void FixedUpdate ()
    {
        currenttime += Time.fixedDeltaTime;
        if (currenttime >= maxtime)
            gameObject.GetComponent<DestroyScript>().Destroyself();
	}

    private void hitTarget(GameObject victim)
    {
        victim.GetComponent<HPScript>().GetHurt(Damage * damageRatio);
        damageRatio -= damageMinus;
        if (damageRatio <= Fix64.Zero)
            gameObject.GetComponent<DestroyScript>().Destroyself();
        bulletRB2D.position = targetRB2D.position;
        currenttime = 0;
    }

    private void OnTriggerEnter2D(Collider2D other)
    {
        if (other.GetComponent<DestroyScript>() != null && other.GetComponent<DestroyScript>().breakable)
        {
            other.GetComponent<DestroyScript>().Destroyself();
            gameObject.GetComponent<DestroyScript>().Destroyself();
            return;
        }
        Target = other.gameObject;
        targetRB2D = Target.GetComponent<Rigidbody2D>();
        hitTarget(other.gameObject);
        GetNextTarget();
    }

    private void GetNextTarget()
    {
        GameObject[] enemies = GameObject.FindGameObjectsWithTag("Player");
        GameObject nextTarget = null;
        float closestDistanceSqr = Mathf.Infinity;
        foreach (GameObject potentialTarget in enemies)
        {
            if (potentialTarget == Target || potentialTarget == sender)
                continue;
            Vector3 directionToTarget = potentialTarget.transform.position - Target.transform.position;
            float dSqrToTarget = directionToTarget.sqrMagnitude;
            if (dSqrToTarget < closestDistanceSqr)
            {
                closestDistanceSqr = dSqrToTarget;
                nextTarget = potentialTarget;
            }
        }
        Target = nextTarget;
        if (Target == null)
            gameObject.GetComponent<DestroyScript>().Destroyself();
        targetRB2D = Target.GetComponent<Rigidbody2D>();
        bulletRB2D.velocity = (targetRB2D.position - bulletRB2D.position).normalized * speed;
    }
}
