using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class SkillE3 : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance = 15;
    public float maxtime = 2;
    public GameObject ST;
    //public GameObject STB;
    float bulletspeed = 5;
    public float damage = 3;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;

    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }

    // Update is called once per frame
    void GoSkillE3()
    {
        if (skillavaliable && GetComponent<DoSkill>().CanSing)
        {
            GetComponent<DoSkill>().singing = 0;
            gameObject.GetComponent<DoSkill>().Fire = Skill;
        }
    }

    private void FixedUpdate()
    {
        if (skillavaliable)
            return;
        if (currentcooldown >= cooldowntime)
        {
            skillavaliable = true;
        }
        else
        {
            currentcooldown += Time.fixedDeltaTime;
            //MyImageScript.IconFillAmount = currentcooldown / cooldowntime;
        }
    }

    public void Skill(Fix64Vector2 actionplacef)
    {
        Vector2 actionplace = actionplacef.ToV2();
        GetComponent<DoSkill>().BeforeSkill();
        Vector2 singplace = transform.position;
        Vector2 skilldirection = actionplace - singplace;
        DoFire(singplace + 0.76f * skilldirection.normalized, skilldirection.normalized * bulletspeed);
        currentcooldown = 0;
        skillavaliable = false;
    }

    void DoFire(Vector2 fireplace, Vector2 speed2d)
    {
        GameObject bullet;
        ST.GetComponent<STScirpt>().sender = gameObject;
        bullet = Instantiate(ST, fireplace, Quaternion.identity);
        bullet.GetComponent<Rigidbody2D>().velocity = speed2d;
        bullet.GetComponent<STScirpt>().maxtime = maxtime;
        bullet.GetComponent<STScirpt>().finalv = (Fix64)0.4 * ((Fix64Vector2)speed2d).normalized();
    }

    void SkillE3SetLevel(int i)
    {
        if (i == 0)
            enabled = false;
        else
            enabled = true;
    }

    public float CalcFA()
    {
        return currentcooldown / cooldowntime;
    }
}
