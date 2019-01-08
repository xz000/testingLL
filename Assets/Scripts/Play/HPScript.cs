using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class HPScript : MonoBehaviour
{
    public Fix64 maxHP;
    public Fix64 currentHP;
    //private GameObject safeground;
    Fix64 outhurt = (Fix64)2;
    bool boost = false;
    public Fix64 boostnow = (Fix64)0;
    public Fix64 boostmax = (Fix64)25;

    // Use this for initialization
    void Start () {
        currentHP = maxHP;
        //safeground = GameObject.Find("GroundCircle");
        boost = false;
	}

    // Update is called once per frame

    /*private void FixedUpdate()
    {
        if (transform.position.magnitude > safeground.transform.lossyScale.x / 2)
        {
            currentHP -= outhurt * Time.fixedDeltaTime;
        }
    }*/
    private void FixedUpdate()
    {
        return;
        if (currentHP <= Fix64.Zero)
        {
            gameObject.GetComponent<DoSkill>().DoClearJob();
            gameObject.GetComponent<DoSkill>().DestroyClean();
            gameObject.GetComponent<DestroyScript>().Destroyself();
        }
        if (currentHP > maxHP)
        {
            currentHP = maxHP;
        }
    }

    private void OnDestroy()
    {
        if (!gameObject.CompareTag("Player"))
            return;
        GameObject[] PlayersLeft = GameObject.FindGameObjectsWithTag("Player");
    }


    public void TransferTo(Vector2 destination)
    {
        GetComponent<Rigidbody2D>().position = destination;
    }

    public void GetHurt(Fix64 damage)
    {
        currentHP -= damage;
        if (boost)
        {
            Fix64 halfdamage = damage / (Fix64)2;
            Fix64 boostvalue = boostmax - boostnow;
            if (boostmax - boostnow >= halfdamage)
                boostvalue = halfdamage;
            currentHP += boostvalue;
            boostnow += boostvalue;
            GetComponent<MoveScript>().movespeed += (float)boostvalue / 50;
        }
    }

    public void boostend()
    {
        boost = false;
        GetComponent<MoveScript>().movespeed -= (float)boostnow / 50;
        boostnow = Fix64.Zero;
    }

    public void booststart()
    {
        boostend();
        boost = true;
    }

    /*public void GetKicked(Vector2 force)
    {
        GetComponent<Rigidbody2D>().AddForce(force);
    }*/
}
