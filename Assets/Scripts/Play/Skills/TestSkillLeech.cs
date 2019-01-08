using System.Collections;
using System.Collections.Generic;
using UnityEngine;
using FixMath;

public class TestSkillLeech : MonoBehaviour
{
    public CooldownImage MyImageScript;
    public float maxdistance;
    public float bulletspeed = 5;
    public GameObject leecher;
    public float damage = 10;
    private float currentcooldown;
    public float cooldowntime = 3;
    public bool skillavaliable;
    
    // Use this for initialization
    void Start()
    {
        currentcooldown = cooldowntime;
    }
    
    // Update is called once per frame
    void GoTestSkillLeech()
    {
        if (skillavaliable)
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

    public void Skill(Fix64Vector2 actionplace)
    {
        //DoSkill.singing = 0;
        //gameObject.GetComponent<DoSkill>().Fire = null;
        GetComponent<DoSkill>().BeforeSkill();
        Fix64 bsf = (Fix64)bulletspeed;
        GameObject bullet;
        Fix64Vector2 skilldirection;
        Fix64Vector2 singplace = (Fix64Vector2)GetComponent<Rigidbody2D>().position;
        skilldirection = actionplace - singplace;
        Fix64 turntime = (skilldirection.Length() - (Fix64)0.76) / bsf;
        leecher.GetComponent<LeechScript>().sender = gameObject;
        leecher.GetComponent<LeechScript>().turntime = (float)turntime;
        leecher.GetComponent<LeechScript>().leechdamage = (Fix64)damage;
        Fix64Vector2 sdnf = skilldirection.normalized();
        bullet = Instantiate(leecher, (singplace + (Fix64)0.76 * sdnf).ToV2(), Quaternion.identity);
        bullet.GetComponent<LeechScript>().speed = bsf;
        bullet.GetComponent<Rigidbody2D>().velocity = (sdnf * bsf).ToV2();
        //bullet.GetComponent<LeechScript>().maxtime = maxdistance / bulletspeed;
        currentcooldown = 0;
        skillavaliable = false;
    }
}
